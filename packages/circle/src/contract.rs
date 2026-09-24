//! Circle contract — core lifecycle implementation.
//!
//! All handlers follow the uniform `&Env`-based API pattern used across the
//! Moistello workspace: every function takes `env: &Env` first and returns a
//! typed `Result<_, CircleError>`. Access control is checked *before* any
//! state mutation (check → compute → write).
//!
//! Security properties implemented here:
//!   * **#324** — `max_members` is bounded to ≤ 128 in `init()` so the
//!     `u128` payout bitmap can never overflow from a `1u128 << pos` shift.
//!   * **#329** — `contribute` only accepts `round == current_round`, and
//!     `trigger_payout` refuses to advance the round until every active
//!     member has contributed (or the round deadline has passed/enforced).
//!   * **#325** — `check_contribution_deadline` auto-assigns strikes to
//!     non-contributors when the deadline passes and auto-triggers payout
//!     when the remaining active members have all complied.
//!   * **#323** — cross-contract calls during mutations (`trigger_payout`,
//!     `batch_payout`, `claim_referral_bonus`, `claim_streak_bonus`) acquire
//!     the canonical `common::reentrancy::ReentrancyGuard`.

use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Vec};

use crate::oracle;
use crate::payout;
use crate::types::*;
use common::math;
use common::pause;
use common::reentrancy::ReentrancyGuard;
use common::vrf;

/// Maximum number of members a circle may hold. Because payout tracking uses
/// a `u128` bitmap (`1u128 << position`), `max_members` must never exceed 128
/// or the shift silently overflows. See issue #324.
pub const MAX_BITMAP_MEMBERS: u32 = 128;

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

fn load_admin(env: &Env) -> Result<Address, CircleError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)
}

fn load_circle(env: &Env) -> Result<Circle, CircleError> {
    env.storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)
}

fn save_circle(env: &Env, circle: &Circle) {
    env.storage().instance().set(&DataKey::Circle, circle);
}

fn load_members(env: &Env) -> Result<Vec<Member>, CircleError> {
    env.storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)
}

fn save_members(env: &Env, members: &Vec<Member>) {
    env.storage().persistent().set(&DataKey::Members, members);
}

fn require_admin(env: &Env, caller: &Address) -> Result<(), CircleError> {
    let admin = load_admin(env)?;
    if caller != &admin {
        return Err(CircleError::Unauthorized);
    }
    caller.require_auth();
    Ok(())
}

/// Admin equality check WITHOUT `require_auth`. Intended for paths that
/// perform their own `require_auth` (e.g. `common::pause`), which would
/// otherwise double-authorize the same frame (`Error(Auth, ExistingValue)`).
fn is_admin(env: &Env, caller: &Address) -> Result<(), CircleError> {
    let admin = load_admin(env)?;
    if caller != &admin {
        return Err(CircleError::Unauthorized);
    }
    Ok(())
}

fn require_not_paused(env: &Env) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)
}

fn round_start(env: &Env, round: u32) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::RoundStart(round))
        .unwrap_or_else(|| env.ledger().timestamp())
}

fn set_round_start(env: &Env, round: u32, ts: u64) {
    env.storage().persistent().set(&DataKey::RoundStart(round), &ts);
}

/// Deadline (ledger timestamp) by which all contributions for `round` are due.
fn round_deadline(env: &Env, circle: &Circle, round: u32) -> u64 {
    round_start(env, round)
        .saturating_add(circle.contribution_deadline_seconds.max(1))
}

/// Whether `member` has already recorded a contribution for `round`.
fn has_contributed(env: &Env, member: &Address, round: u32) -> bool {
    let contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundContributionRecords(round))
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..contributions.len() {
        if contributions
            .get(i)
            .map(|contribution| contribution.member == *member)
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

/// Count of active members who still need to contribute for `round`.
fn active_members_remaining(
    env: &Env,
    members: &Vec<Member>,
    round: u32,
) -> u32 {
    let mut remaining: u32 = 0;
    for i in 0..members.len() {
        if let Some(m) = members.get(i) {
            if m.status == MEMBER_ACTIVE && !has_contributed(env, &m.address, round) {
                remaining = remaining.saturating_add(1);
            }
        }
    }
    remaining
}

fn is_active_member(members: &Vec<Member>, addr: &Address) -> bool {
    for i in 0..members.len() {
        if let Some(m) = members.get(i) {
            if m.address == *addr && m.status == MEMBER_ACTIVE {
                return true;
            }
        }
    }
    false
}

fn get_position(members: &Vec<Member>, addr: &Address) -> Option<u32> {
    for i in 0..members.len() {
        if let Some(m) = members.get(i) {
            if m.address == *addr {
                return Some(m.position);
            }
        }
    }
    None
}

fn all_active_positions_paid(members: &Vec<Member>, payout_bitmap: u128) -> bool {
    let mut active = false;
    for i in 0..members.len() {
        if let Some(member) = members.get(i) {
            if member.status == MEMBER_ACTIVE {
                active = true;
                if member.position >= MAX_BITMAP_MEMBERS
                    || payout_bitmap & (1u128 << member.position) == 0
                {
                    return false;
                }
            }
        }
    }
    active
}

fn token_client<'a>(env: &'a Env) -> Result<TokenClient<'a>, CircleError> {
    let circle = load_circle(env)?;
    Ok(TokenClient::new(env, &circle.token))
}

// ---------------------------------------------------------------------------
// Init & configuration
// ---------------------------------------------------------------------------

pub fn init(
    env: &Env,
    admin: &Address,
    factory: &Address,
    config: &CircleConfig,
) -> Result<(), CircleError> {
    if env.storage().instance().has(&DataKey::Circle) {
        return Err(CircleError::AlreadyContributed);
    }

    // #324 — the payout bitmap is a u128; positions must stay < 128.
    if config.max_members == 0 || config.max_members > MAX_BITMAP_MEMBERS {
        return Err(CircleError::InvalidMaxMembers);
    }
    if config.contribution_amount <= 0 {
        return Err(CircleError::InvalidAmount);
    }
    if config.total_rounds == 0 {
        return Err(CircleError::InvalidRound);
    }
    if config.max_strikes == 0 {
        return Err(CircleError::InvalidMemberStatus);
    }

    let now = env.ledger().timestamp();
    let circle = Circle {
        id: env.current_contract_address(),
        token: config.token.clone(),
        name: config.name.clone(),
        organizer: config.organizer.clone(),
        factory: factory.clone(),
        contribution_amount: config.contribution_amount,
        max_members: config.max_members,
        member_count: 0,
        payout_type: config.payout_type,
        total_rounds: config.total_rounds,
        current_round: 0,
        status: STATUS_PENDING,
        started_at: 0,
        created_at: now,
        contribution_deadline_seconds: config.contribution_deadline_seconds,
        min_moi_score: config.min_moi_score,
        collateral_amount: config.collateral_amount,
        penalty_bps: config.penalty_bps,
        grace_period_seconds: config.grace_period_seconds,
        max_strikes: config.max_strikes,
        payout_bitmap: 0,
        total_payouts: 0,
        total_fees: 0,
        slug: config.slug.clone(),
    };

    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::Factory, factory);
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().instance().set(&DataKey::FeeBps, &0u32);
    env.storage().instance().set(&DataKey::Treasury, &Option::<Address>::None);
    save_members(env, &Vec::new(env));
    env.storage().persistent().set(&DataKey::Bids, &Vec::<AuctionBid>::new(env));
    env.storage().persistent().set(&DataKey::Votes, &Vec::<VoteEntry>::new(env));
    env.storage().persistent().set(&DataKey::Payouts, &Vec::<PayoutRecipient>::new(env));
    env.storage().persistent().set(&DataKey::Contributions, &Vec::<Contribution>::new(env));
    env.storage().persistent().set(&DataKey::Referrals, &Vec::<Referral>::new(env));
    set_round_start(env, 0, now);

    // Random payouts need the VRF seeded once. Idempotent per contract.
    let _ = vrf::init_vrf(env, None);

    Ok(())
}

pub fn set_fee_bps(env: &Env, admin: &Address, fee_bps: u32) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    if fee_bps > 10_000 {
        return Err(CircleError::InvalidAmount);
    }
    env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
    Ok(())
}

pub fn set_treasury(env: &Env, admin: &Address, treasury: &Address) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    env.storage()
        .instance()
        .set(&DataKey::Treasury, &Option::<Address>::Some(treasury.clone()));
    Ok(())
}

pub fn set_token(env: &Env, admin: &Address, token: &Address) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    let mut circle = load_circle(env)?;
    circle.token = token.clone();
    save_circle(env, &circle);
    Ok(())
}

pub fn set_reputation_registry(
    env: &Env,
    admin: &Address,
    registry: &Address,
) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    env.storage().instance().set(&DataKey::ReputationRegistry, registry);
    Ok(())
}

pub fn get_reputation_registry(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::ReputationRegistry)
}

pub fn set_allowlist(
    env: &Env,
    admin: &Address,
    allowlist: Vec<Address>,
) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    env.storage().persistent().set(&DataKey::Allowlist, &allowlist);
    Ok(())
}

pub fn get_allowlist(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::Allowlist)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_oracle(env: &Env, admin: &Address, oracle: &Address) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    oracle::set_primary_oracle(env, oracle);
    Ok(())
}

pub fn set_fallback_oracle(
    env: &Env,
    admin: &Address,
    oracle: &Address,
) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    oracle::set_fallback_oracle(env, oracle);
    Ok(())
}

pub fn get_oracle(env: &Env) -> Option<Address> {
    oracle::get_primary_oracle(env)
}

pub fn get_fallback_oracle(env: &Env) -> Option<Address> {
    oracle::get_fallback_oracle(env)
}

// ---------------------------------------------------------------------------
// Membership
// ---------------------------------------------------------------------------

pub fn join(env: &Env, member: &Address) -> Result<(), CircleError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    require_not_paused(env)?;
    let mut circle = load_circle(env)?;
    if circle.status != STATUS_PENDING && circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if circle.member_count >= circle.max_members {
        return Err(CircleError::CircleFull);
    }
    let allowlist = get_allowlist(env);
    if allowlist.len() > 0 {
        let mut permitted = false;
        for i in 0..allowlist.len() {
            if allowlist.get(i).map(|a| a == *member).unwrap_or(false) {
                permitted = true;
                break;
            }
        }
        if !permitted {
            return Err(CircleError::AllowlistNotPermitted);
        }
    }
    let members = load_members(env)?;
    if is_active_member(&members, member) {
        return Err(CircleError::AlreadyMember);
    }

    member.require_auth();

    // Collect collateral up-front (check → compute → write).
    if circle.collateral_amount > 0 {
        let contract = env.current_contract_address();
        token_client(env)?.transfer(member, &contract, &circle.collateral_amount);
    }

    let position = circle.member_count;
    let ts = env.ledger().timestamp();
    let mut members = members;
    members.push_back(Member {
        address: member.clone(),
        position,
        joined_at: ts,
        strikes: 0,
        status: MEMBER_ACTIVE,
        exited_at: 0,
        total_contributions: 0,
        total_received: 0,
    });
    save_members(env, &members);

    circle.member_count = circle.member_count.saturating_add(1);
    if circle.member_count == circle.max_members {
        circle.status = STATUS_ACTIVE;
        circle.started_at = ts;
    }
    save_circle(env, &circle);

    env.events().publish(
        (env.current_contract_address(), symbol_short!("joined")),
        MemberJoined {
            member: member.clone(),
            position,
        },
    );
    Ok(())
}

pub fn batch_invite(
    env: &Env,
    caller: &Address,
    members: &Vec<Address>,
) -> Result<(), CircleError> {
    require_admin(env, caller)?;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        join_internal(env, &m)?;
    }
    Ok(())
}

fn join_internal(env: &Env, member: &Address) -> Result<(), CircleError> {
    let mut circle = load_circle(env)?;
    if circle.member_count >= circle.max_members {
        return Err(CircleError::CircleFull);
    }
    let members = load_members(env)?;
    if is_active_member(&members, member) {
        return Err(CircleError::AlreadyMember);
    }
    let position = circle.member_count;
    let ts = env.ledger().timestamp();
    let mut members = members;
    members.push_back(Member {
        address: member.clone(),
        position,
        joined_at: ts,
        strikes: 0,
        status: MEMBER_ACTIVE,
        exited_at: 0,
        total_contributions: 0,
        total_received: 0,
    });
    save_members(env, &members);
    circle.member_count = circle.member_count.saturating_add(1);
    if circle.member_count == circle.max_members {
        circle.status = STATUS_ACTIVE;
        circle.started_at = ts;
    }
    save_circle(env, &circle);
    Ok(())
}

// ---------------------------------------------------------------------------
// Contributions
// ---------------------------------------------------------------------------

pub fn contribute(
    env: &Env,
    member: &Address,
    amount: i128,
    round: u32,
) -> Result<(), CircleError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    require_not_paused(env)?;
    let mut circle = load_circle(env)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    // #329 — a contribution may only target the currently-open round.
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if round >= circle.total_rounds {
        return Err(CircleError::InvalidRound);
    }
    if amount != circle.contribution_amount {
        return Err(CircleError::ContributionMismatch);
    }
    let members = load_members(env)?;
    if !is_active_member(&members, member) {
        return Err(CircleError::NotMember);
    }
    if has_contributed(env, member, round) {
        return Err(CircleError::AlreadyContributed);
    }

    member.require_auth();

    // Pull the contribution into the pool (check → compute → write).
    let contract = env.current_contract_address();
    token_client(env)?.transfer(member, &contract, &amount);

    let now = env.ledger().timestamp();
    let deadline = round_deadline(env, &circle, round);
    let on_time = now <= deadline;
    let elapsed = deadline.saturating_sub(now);
    let time_weight: u64 = if on_time {
        elapsed.max(1) as u64
    } else {
        1u64
    };

    let contribution = Contribution {
        member: member.clone(),
        round,
        amount,
        timestamp: now,
        on_time,
        time_weight,
    };
    let mut round_contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundContributionRecords(round))
        .unwrap_or_else(|| Vec::new(env));
    round_contributions.push_back(contribution);
    env.storage().persistent().set(
        &DataKey::RoundContributionRecords(round),
        &round_contributions,
    );
    let round_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::RoundContributions(round))
        .unwrap_or(0);
    env.storage()
        .persistent()
        .set(&DataKey::RoundContributions(round), &round_count.saturating_add(1));
    let round_pool: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::RoundPool(round))
        .unwrap_or(0);
    env.storage().persistent().set(
        &DataKey::RoundPool(round),
        &round_pool.saturating_add(amount),
    );

    let mut members = members;
    for i in 0..members.len() {
        if let Some(m) = members.get(i) {
            if m.address == *member {
                let mut updated = m;
                updated.total_contributions = updated
                    .total_contributions
                    .saturating_add(amount);
                updated.strikes = 0; // a contribution clears the round's strike
                members.set(i, updated);
                break;
            }
        }
    }
    save_members(env, &members);

    // Track referred member's cumulative contribution for referral bonuses.
    let referred_total: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::Contribution(member.clone()))
        .unwrap_or(0);
    env.storage()
        .persistent()
        .set(
            &DataKey::Contribution(member.clone()),
            &referred_total.saturating_add(amount),
        );

    env.events().publish(
        (env.current_contract_address(), symbol_short!("contrib")),
        ContributionRecorded {
            member: member.clone(),
            round,
            amount,
            on_time,
        },
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Deadline enforcement (#325)
// ---------------------------------------------------------------------------

/// Enforces the contribution deadline for `circle.current_round`:
///   * auto-assigns a strike to every active non-contributor,
///   * marks members whose strikes reach `max_strikes` as defaulted,
///   * emits `MemberDefaulted` for every auto-assigned strike,
///   * auto-triggers the payout when all remaining active members complied.
///
/// Idempotent per round — calling it more than once for the same round is a
/// no-op.
pub fn check_contribution_deadline(env: &Env) -> Result<(), CircleError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    let circle = load_circle(env)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    let round = circle.current_round;
    if round >= circle.total_rounds {
        return Err(CircleError::InvalidRound);
    }
    let now = env.ledger().timestamp();
    let deadline = round_deadline(env, &circle, round);
    if now < deadline {
        return Err(CircleError::DeadlineNotPassed);
    }
    if env
        .storage()
        .persistent()
        .get(&DataKey::RoundEnforced(round))
        .unwrap_or(false)
    {
        return Ok(());
    }

    let mut members = load_members(env)?;
    let mut any_strike = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status != MEMBER_ACTIVE && m.status != MEMBER_DEFAULTED {
            continue;
        }
        if m.status == MEMBER_ACTIVE && has_contributed(env, &m.address, round) {
            continue;
        }
        // Only members that were supposed to contribute get strikes. A member
        // already defaulted before this round keeps their status.
        if m.status == MEMBER_DEFAULTED {
            continue;
        }
        any_strike = true;
        let addr = m.address.clone();
        let mut updated = m;
        updated.strikes = updated.strikes.saturating_add(1);
        if updated.strikes >= circle.max_strikes {
            updated.status = MEMBER_DEFAULTED;
        }
        let new_strikes = updated.strikes;
        members.set(i, updated);
        env.events().publish(
            (env.current_contract_address(), symbol_short!("default")),
            MemberDefaulted {
                member: addr,
                strikes: new_strikes,
            },
        );
    }

    if any_strike {
        save_members(env, &members);
    }

    env.storage()
        .persistent()
        .set(&DataKey::RoundEnforced(round), &true);

    // Deadline enforcement itself authorizes the round to resolve. Members
    // that missed the deadline remain active until their strike limit is
    // reached, so waiting for zero remaining contributors here would leave
    // the circle permanently stuck after a partial default.
    let _ = trigger_payout_internal(env, round)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Payouts
// ---------------------------------------------------------------------------

pub fn trigger_payout(
    env: &Env,
    caller: &Address,
    round: u32,
) -> Result<(), CircleError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    require_admin(env, caller)?;
    trigger_payout_internal(env, round)
}

fn trigger_payout_internal(env: &Env, round: u32) -> Result<(), CircleError> {
    require_not_paused(env)?;
    let mut circle = load_circle(env)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if round >= circle.total_rounds {
        return Err(CircleError::InvalidRound);
    }

    let members = load_members(env)?;

    // The bitmap tracks one payout per active member. Once every active
    // position has been paid, begin the next payout cycle with a fresh map.
    let fixed_cycle_boundary = circle.payout_type == PAYOUT_FIXED
        && round > 0
        && round % circle.max_members == 0;
    if fixed_cycle_boundary || all_active_positions_paid(&members, circle.payout_bitmap) {
        circle.payout_bitmap = 0;
    }

    // #329 — block round advancement until every active member has
    // contributed, unless the deadline has passed (and been enforced).
    let deadline = round_deadline(env, &circle, round);
    let now = env.ledger().timestamp();
    let deadline_passed = now >= deadline;
    let enforced = env
        .storage()
        .persistent()
        .get(&DataKey::RoundEnforced(round))
        .unwrap_or(false);
    if active_members_remaining(env, &members, round) != 0 && !(deadline_passed || enforced) {
        return Err(CircleError::InvalidContributionRound);
    }

    // Resolve the recipient for this round's payout type.
    let recipient = resolve_round_recipient(env, &circle, &members, round)?;
    let position = get_position(&members, &recipient).ok_or(CircleError::NotMember)?;

    // Compute the pool from recorded contributions for this round.
    let pool = pool_for_round(env, round);
    if pool <= 0 {
        return Err(CircleError::ZeroPayoutAmount);
    }
    let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
    let (net, fee) = math::apply_fee(pool, fee_bps as i128)
        .map_err(|_| CircleError::InvalidAmount)?;

    let contract = env.current_contract_address();
    let token = token_client(env)?;

    // Transfer net payout to the winner.
    if net > 0 {
        token.transfer(&contract, &recipient, &net);
    }
    // Deposit the fee into the configured treasury. The circle transfers the
    // fee tokens directly (Soroban auth is non-transferable: an intermediary
    // cannot pull tokens on the circle's behalf), then asks the treasury to
    // record the swept deposit. If the treasury is not deployed the tokens are
    // still delivered — collection is never lost.
    if fee > 0 {
        let treasury: Option<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Treasury)
            .unwrap_or(None);
        if let Some(treasury_addr) = treasury {
            token.transfer(&contract, &treasury_addr, &fee);
            let treasury_client = treasury::TreasuryClient::new(env, &treasury_addr);
            let _ = treasury_client.try_sweep_fee(&contract, &fee, &contract);
        }
    }

    // Mark the recipient's position as paid in the bitmap.
    circle.payout_bitmap |= 1u128 << position;
    circle.total_payouts = circle.total_payouts.saturating_add(net);
    circle.total_fees = circle.total_fees.saturating_add(fee);

    let mut payouts: Vec<PayoutRecipient> = env
        .storage()
        .persistent()
        .get(&DataKey::Payouts)
        .unwrap_or_else(|| Vec::new(env));
    payouts.push_back(PayoutRecipient {
        recipient: recipient.clone(),
        round,
        amount: net,
        fee,
        payout_type: circle.payout_type,
        timestamp: env.ledger().timestamp(),
    });
    env.storage().persistent().set(&DataKey::Payouts, &payouts);

    // Record total received on the member.
    let mut members = members;
    for i in 0..members.len() {
        if let Some(m) = members.get(i) {
            if m.address == recipient {
                let mut updated = m;
                updated.total_received = updated.total_received.saturating_add(net);
                members.set(i, updated);
                break;
            }
        }
    }
    save_members(env, &members);

    circle.current_round = circle.current_round.saturating_add(1);
    if circle.current_round >= circle.total_rounds {
        circle.status = STATUS_COMPLETED;
    }
    save_circle(env, &circle);

    // Track per-round completion time for the next round's deadline.
    let next_round = circle.current_round;
    set_round_start(env, next_round, env.ledger().timestamp());

    env.events().publish(
        (env.current_contract_address(), symbol_short!("payout")),
        PayoutExecuted {
            recipient: recipient.clone(),
            round,
            amount: net,
            fee,
            payout_type: circle.payout_type,
            timestamp: env.ledger().timestamp(),
        },
    );

    if circle.status == STATUS_COMPLETED {
        env.events().publish(
            (env.current_contract_address(), symbol_short!("compl")),
            CircleCompleted {
                total_payouts: circle.total_payouts,
            },
        );
    }

    Ok(())
}

pub fn batch_payout(
    env: &Env,
    caller: &Address,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
    round: u32,
) -> Result<(), CircleError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    require_admin(env, caller)?;
    require_not_paused(env)?;
    let mut circle = load_circle(env)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if recipients.len() != amounts.len() {
        return Err(CircleError::InvalidAmount);
    }
    if recipients.len() == 0 || recipients.len() > 10 {
        return Err(CircleError::InvalidAmount);
    }

    let mut total: i128 = 0;
    for i in 0..amounts.len() {
        let a = amounts.get(i).ok_or(CircleError::VecAccessError)?;
        if a <= 0 {
            return Err(CircleError::InvalidAmount);
        }
        total = total
            .checked_add(a)
            .ok_or(CircleError::InvalidAmount)?;
    }

    let contract = env.current_contract_address();
    let token = token_client(env)?;
    let balance = token.balance(&contract);
    if total > balance {
        return Err(CircleError::InsufficientContractBalance);
    }

    for i in 0..recipients.len() {
        let to = recipients.get(i).ok_or(CircleError::VecAccessError)?;
        let amount = amounts.get(i).ok_or(CircleError::VecAccessError)?;
        if amount > 0 {
            token.transfer(&contract, &to, &amount);
        }
    }

    circle.total_payouts = circle.total_payouts.saturating_add(total);
    save_circle(env, &circle);

    env.events().publish(
        (env.current_contract_address(), symbol_short!("bpayout")),
        PayoutExecuted {
            recipient: recipients
                .get(recipients.len().saturating_sub(1))
                .unwrap_or(contract),
            round,
            amount: total,
            fee: 0,
            payout_type: circle.payout_type,
            timestamp: env.ledger().timestamp(),
        },
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Payout resolution helpers
// ---------------------------------------------------------------------------

fn pool_for_round(env: &Env, round: u32) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::RoundPool(round))
        .unwrap_or(0)
}

fn resolve_round_recipient(
    env: &Env,
    circle: &Circle,
    _members: &Vec<Member>,
    round: u32,
) -> Result<Address, CircleError> {
    match circle.payout_type {
        PAYOUT_FIXED => payout::resolve_fixed(env, circle, round),
        PAYOUT_AUCTION => {
            let (addr, _bips) = payout::resolve_auction(env, circle, round)?;
            Ok(addr)
        }
        PAYOUT_VOTE => payout::resolve_vote(env, circle, round),
        _ => payout::resolve_random(env, circle, round),
    }
}

// ---------------------------------------------------------------------------
// Auctions & votes
// ---------------------------------------------------------------------------

pub fn auction_bid(
    env: &Env,
    bidder: &Address,
    discount_bips: u32,
    round: u32,
) -> Result<(), CircleError> {
    require_not_paused(env)?;
    let circle = load_circle(env)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if circle.payout_type != PAYOUT_AUCTION {
        return Err(CircleError::InvalidPayoutType);
    }
    if discount_bips > 10_000 {
        return Err(CircleError::InvalidBid);
    }
    let members = load_members(env)?;
    if !is_active_member(&members, bidder) {
        return Err(CircleError::NotMember);
    }

    let bids: Vec<AuctionBid> = env
        .storage()
        .persistent()
        .get(&DataKey::Bids)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..bids.len() {
        if let Some(b) = bids.get(i) {
            if b.bidder == *bidder && b.round == round {
                return Err(CircleError::AlreadyBidded);
            }
        }
    }

    bidder.require_auth();
    let mut bids = bids;
    bids.push_back(AuctionBid {
        bidder: bidder.clone(),
        discount_bips,
        round,
        timestamp: env.ledger().timestamp(),
    });
    env.storage().persistent().set(&DataKey::Bids, &bids);

    env.events().publish(
        (env.current_contract_address(), symbol_short!("bid")),
        AuctionBidPlaced {
            bidder: bidder.clone(),
            discount_bips,
            round,
        },
    );
    Ok(())
}

pub fn vote_payout(
    env: &Env,
    voter: &Address,
    vote_for: &Address,
    round: u32,
) -> Result<(), CircleError> {
    require_not_paused(env)?;
    let circle = load_circle(env)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if circle.payout_type != PAYOUT_VOTE {
        return Err(CircleError::InvalidPayoutType);
    }
    let members = load_members(env)?;
    if !is_active_member(&members, voter) {
        return Err(CircleError::NotMember);
    }
    if !is_active_member(&members, vote_for) {
        return Err(CircleError::NotMember);
    }

    let votes: Vec<VoteEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::Votes)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..votes.len() {
        if let Some(v) = votes.get(i) {
            if v.voter == *voter && v.round == round {
                return Err(CircleError::AlreadyVoted);
            }
        }
    }

    voter.require_auth();
    let mut votes = votes;
    votes.push_back(VoteEntry {
        voter: voter.clone(),
        vote_for: vote_for.clone(),
        round,
        timestamp: env.ledger().timestamp(),
    });
    env.storage().persistent().set(&DataKey::Votes, &votes);

    env.events().publish(
        (env.current_contract_address(), symbol_short!("vote")),
        VoteCast {
            voter: voter.clone(),
            vote_for: vote_for.clone(),
            round,
        },
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Late reporting
// ---------------------------------------------------------------------------

pub fn report_late(
    env: &Env,
    reporter: &Address,
    late_member: &Address,
    round: u32,
) -> Result<(), CircleError> {
    require_not_paused(env)?;
    let circle = load_circle(env)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    let members = load_members(env)?;
    if !is_active_member(&members, reporter) {
        return Err(CircleError::NotMember);
    }

    // Locate the contribution; it must exist and be flagged late.
    let contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundContributionRecords(round))
        .unwrap_or_else(|| Vec::new(env));
    let contribution = (0..contributions.len())
        .find_map(|index| {
            contributions
                .get(index)
                .filter(|candidate| candidate.member == *late_member)
        })
        .ok_or(CircleError::ContributionNotFound)?;
    if contribution.on_time {
        return Err(CircleError::ContributionNotFound);
    }

    reporter.require_auth();

    let mut members = members;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *late_member {
            let mut updated = m;
            updated.strikes = updated.strikes.saturating_add(1);
            if updated.strikes >= circle.max_strikes {
                updated.status = MEMBER_DEFAULTED;
            }
            let new_strikes = updated.strikes;
            members.set(i, updated);
            env.events().publish(
                (env.current_contract_address(), symbol_short!("default")),
                MemberDefaulted {
                    member: late_member.clone(),
                    strikes: new_strikes,
                },
            );
            save_members(env, &members);
            return Ok(());
        }
    }
    Err(CircleError::NotMember)
}

// ---------------------------------------------------------------------------
// Exit & cancel
// ---------------------------------------------------------------------------

pub fn exit(env: &Env, member: &Address) -> Result<(), CircleError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    require_not_paused(env)?;
    let circle = load_circle(env)?;
    if circle.status != STATUS_PENDING && circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    let members = load_members(env)?;
    let mut found_idx: Option<u32> = None;
    let mut member_record: Option<Member> = None;
    let mut exited = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *member {
            let mut updated = m;
            if updated.status == MEMBER_ACTIVE {
                exited = true;
            }
            updated.status = MEMBER_EXITED;
            updated.exited_at = env.ledger().timestamp();
            member_record = Some(updated);
            found_idx = Some(i);
            break;
        }
    }
    // Exiting a circle where the caller was never a member is a no-op
    // (matches established test semantics).
    if member_record.is_none() || !exited {
        return Ok(());
    }
    let idx = found_idx.ok_or(CircleError::VecAccessError)?;

    member.require_auth();

    let mut members = members;
    let rec = member_record.ok_or(CircleError::NotMember)?;
    members.set(idx, rec.clone());
    save_members(env, &members);

    // Refund collateral, minus penalty, when collateral is staked.
    if circle.collateral_amount > 0 {
        let penalty = math::calculate_penalty(circle.collateral_amount, circle.penalty_bps as i128)
            .unwrap_or(0);
        let refund = circle.collateral_amount.saturating_sub(penalty);
        if refund > 0 {
            let contract = env.current_contract_address();
            token_client(env)?.transfer(&contract, member, &refund);
        }
    }

    env.events().publish(
        (env.current_contract_address(), symbol_short!("exit")),
        MemberExited {
            member: member.clone(),
            penalty: 0,
        },
    );
    Ok(())
}

pub fn cancel_circle(env: &Env, caller: &Address) -> Result<(), CircleError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    require_admin(env, caller)?;
    require_not_paused(env)?;
    let mut circle = load_circle(env)?;
    if circle.status == STATUS_COMPLETED || circle.status == STATUS_CANCELLED {
        return Err(CircleError::NotActive);
    }

    caller.require_auth();

    // Return the current pool to the members (best-effort proportional).
    let contract = env.current_contract_address();
    let token = token_client(env)?;
    let balance = token.balance(&contract);
    let members = load_members(env)?;
    let active = (0..members.len())
        .filter(|&i| {
            members
                .get(i)
                .map(|m| m.status == MEMBER_ACTIVE)
                .unwrap_or(false)
        })
        .count() as i128;

    circle.status = STATUS_CANCELLED;
    if balance > 0 && active > 0 {
        let per_member = balance / active;
        if per_member > 0 {
            for i in 0..members.len() {
                if let Some(m) = members.get(i) {
                    if m.status == MEMBER_ACTIVE {
                        token.transfer(&contract, &m.address, &per_member);
                    }
                }
            }
        }
    }
    save_circle(env, &circle);

    env.events().publish(
        (env.current_contract_address(), symbol_short!("cancel")),
        CircleCancelled {
            circle_id: circle.id.clone(),
            cancelled_by: caller.clone(),
            cancelled_at: env.ledger().timestamp(),
        },
    );
    Ok(())
}

pub fn cancel(env: &Env, caller: &Address) -> Result<(), CircleError> {
    cancel_circle(env, caller)
}

// ---------------------------------------------------------------------------
// Disputes
// ---------------------------------------------------------------------------

pub fn dispute(
    env: &Env,
    member: &Address,
    evidence_hash: &BytesN<32>,
) -> Result<(), CircleError> {
    raise_dispute(env, member, evidence_hash)
}

pub fn raise_dispute(
    env: &Env,
    member: &Address,
    evidence_hash: &BytesN<32>,
) -> Result<(), CircleError> {
    require_not_paused(env)?;
    let circle = load_circle(env)?;
    if circle.status == STATUS_COMPLETED || circle.status == STATUS_CANCELLED {
        return Err(CircleError::NotActive);
    }
    if env.storage().persistent().has(&DataKey::Dispute) {
        return Err(CircleError::DisputeAlreadyRaised);
    }

    member.require_auth();

    let mut circle = circle;
    circle.status = STATUS_DISPUTED;
    save_circle(env, &circle);
    env.storage().persistent().set(
        &DataKey::Dispute,
        &DisputeEntry {
            raised_by: member.clone(),
            evidence_hash: evidence_hash.clone(),
            raised_at: env.ledger().timestamp(),
            resolved_at: 0,
            resolution: 0,
            resolved_by: circle.organizer.clone(),
        },
    );

    env.events().publish(
        (env.current_contract_address(), symbol_short!("dispute")),
        DisputeRaised {
            member: member.clone(),
            evidence_hash: evidence_hash.clone(),
        },
    );
    Ok(())
}

pub fn resolve_dispute(env: &Env, admin: &Address, resolution: u32) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    let dispute: DisputeEntry = env
        .storage()
        .persistent()
        .get(&DataKey::Dispute)
        .ok_or(CircleError::NoActiveDispute)?;
    if resolution != RESOLVE_DISMISS
        && resolution != RESOLVE_PENALIZE
        && resolution != RESOLVE_FORCE_PAYOUT
    {
        return Err(CircleError::InvalidMemberStatus);
    }

    let mut circle = load_circle(env)?;
    circle.status = STATUS_ACTIVE;
    save_circle(env, &circle);

    env.storage().persistent().set(
        &DataKey::Dispute,
        &DisputeEntry {
            raised_by: dispute.raised_by.clone(),
            evidence_hash: dispute.evidence_hash.clone(),
            raised_at: dispute.raised_at,
            resolved_at: env.ledger().timestamp(),
            resolution,
            resolved_by: admin.clone(),
        },
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Referrals & streaks
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug)]
struct ReferralStorage {
    pub bonus_pct: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
struct StreakConfigStorage {
    pub base_bonus: i128,
    pub multiplier_per_day: i128,
    pub min_streak: u32,
}

pub fn register_referral(
    env: &Env,
    referrer: &Address,
    referred: &Address,
    bonus_pct: u32,
) -> Result<(), CircleError> {
    require_not_paused(env)?;
    if referrer == referred {
        return Err(CircleError::SelfReferral);
    }
    if bonus_pct > 10_000 {
        return Err(CircleError::InvalidBid);
    }
    let members = load_members(env)?;
    if !is_active_member(&members, referrer) {
        return Err(CircleError::NotMember);
    }
    if !is_active_member(&members, referred) {
        return Err(CircleError::NotMember);
    }

    referrer.require_auth();

    let mut referrals: Vec<Referral> = env
        .storage()
        .persistent()
        .get(&DataKey::Referrals)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..referrals.len() {
        if let Some(r) = referrals.get(i) {
            if r.referred == *referred {
                return Err(CircleError::AlreadyMember);
            }
        }
    }
    referrals.push_back(Referral {
        referrer: referrer.clone(),
        referred: referred.clone(),
        bonus_pct,
        timestamp: env.ledger().timestamp(),
    });
    env.storage().persistent().set(&DataKey::Referrals, &referrals);

    env.events().publish(
        (env.current_contract_address(), symbol_short!("refer")),
        ReferralRegistered {
            referrer: referrer.clone(),
            referred: referred.clone(),
            bonus_pct,
        },
    );
    Ok(())
}

pub fn claim_referral_bonus(env: &Env, referrer: &Address) -> Result<(), CircleError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    require_not_paused(env)?;
    referrer.require_auth();

    let cfg: ReferralStorage = env
        .storage()
        .persistent()
        .get(&DataKey::ReferralConfig)
        .unwrap_or(ReferralStorage { bonus_pct: 500 });

    let referrals: Vec<Referral> = env
        .storage()
        .persistent()
        .get(&DataKey::Referrals)
        .unwrap_or_else(|| Vec::new(env));

    let mut total_bonus: i128 = 0;
    let mut claimed_any = false;
    for i in 0..referrals.len() {
        let r = referrals.get(i).ok_or(CircleError::VecAccessError)?;
        if r.referrer != *referrer {
            continue;
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::ReferralClaimed(referrer.clone(), r.referred.clone()))
        {
            continue;
        }
        let contributed: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Contribution(r.referred.clone()))
            .unwrap_or(0);
        let pct = if r.bonus_pct != 0 {
            r.bonus_pct
        } else {
            cfg.bonus_pct
        };
        let bonus = math::calculate_percentage(contributed, pct as i128)
            .map_err(|_| CircleError::InvalidAmount)?;
        if bonus > 0 {
            total_bonus = total_bonus.saturating_add(bonus);
            claimed_any = true;
        }
        env.storage().persistent().set(
            &DataKey::ReferralClaimed(referrer.clone(), r.referred.clone()),
            &true,
        );
    }
    if !claimed_any || total_bonus <= 0 {
        return Err(CircleError::ZeroPayoutAmount);
    }

    let contract = env.current_contract_address();
    let token = token_client(env)?;
    if token.balance(&contract) < total_bonus {
        return Err(CircleError::InsufficientContractBalance);
    }
    token.transfer(&contract, referrer, &total_bonus);
    Ok(())
}

pub fn get_referrals(env: &Env) -> Vec<Referral> {
    env.storage()
        .persistent()
        .get(&DataKey::Referrals)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn update_streak(
    env: &Env,
    member: &Address,
    round: u32,
) -> Result<(), CircleError> {
    require_not_paused(env)?;
    member.require_auth();
    let members = load_members(env)?;
    if !is_active_member(&members, member) {
        return Err(CircleError::NotMember);
    }

    let mut streak: Streak = env
        .storage()
        .persistent()
        .get(&DataKey::Streak(member.clone()))
        .unwrap_or(Streak {
            member: member.clone(),
            current_streak: 0,
            longest_streak: 0,
            last_round: 0,
        });

    if streak.last_round.saturating_add(1) == round {
        streak.current_streak = streak.current_streak.saturating_add(1);
    } else if streak.last_round < round {
        streak.current_streak = 1;
    }
    if streak.current_streak > streak.longest_streak {
        streak.longest_streak = streak.current_streak;
    }
    streak.last_round = round;
    env.storage()
        .persistent()
        .set(&DataKey::Streak(member.clone()), &streak);
    Ok(())
}

pub fn claim_streak_bonus(env: &Env, member: &Address) -> Result<(), CircleError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::ReentrantCall)?;
    require_not_paused(env)?;
    member.require_auth();

    let streak: Streak = env
        .storage()
        .persistent()
        .get(&DataKey::Streak(member.clone()))
        .ok_or(CircleError::NotMember)?;

    let cfg: StreakConfigStorage = env
        .storage()
        .instance()
        .get(&DataKey::StreakConfig)
        .unwrap_or(StreakConfigStorage {
            base_bonus: 100_0000,
            multiplier_per_day: 10_0000,
            min_streak: 3,
        });
    if streak.current_streak < cfg.min_streak {
        return Err(CircleError::InvalidMemberStatus);
    }

    // Cooldown: only one claim per round.
    let current_round: u32 = load_circle(env)?.current_round;
    let last_claimed: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::StreakLastClaimedRound(member.clone()))
        .unwrap_or(0);
    if last_claimed >= current_round {
        return Err(CircleError::AlreadyVoted);
    }
    env.storage()
        .persistent()
        .set(
            &DataKey::StreakLastClaimedRound(member.clone()),
            &current_round,
        );

    let bonus = cfg
        .base_bonus
        .saturating_add((streak.current_streak as i128).saturating_mul(cfg.multiplier_per_day));
    let contract = env.current_contract_address();
    let token = token_client(env)?;
    if token.balance(&contract) < bonus {
        return Err(CircleError::InsufficientContractBalance);
    }
    token.transfer(&contract, member, &bonus);

    env.storage().persistent().set(&DataKey::Streak(member.clone()), &streak);
    Ok(())
}

pub fn get_streaks(env: &Env) -> Vec<Streak> {
    let members = load_members(env).unwrap_or_else(|_| Vec::new(env));
    let mut out = Vec::new(env);
    for i in 0..members.len() {
        if let Some(m) = members.get(i) {
            let streak: Option<Streak> = env
                .storage()
                .persistent()
                .get(&DataKey::Streak(m.address.clone()));
            if let Some(s) = streak {
                out.push_back(s);
            }
        }
    }
    out
}

pub fn get_member_streak(env: &Env, member: &Address) -> Streak {
    env.storage()
        .persistent()
        .get(&DataKey::Streak(member.clone()))
        .unwrap_or(Streak {
            member: member.clone(),
            current_streak: 0,
            longest_streak: 0,
            last_round: 0,
        })
}

// ---------------------------------------------------------------------------
// Getters
// ---------------------------------------------------------------------------

pub fn get_status(env: &Env) -> Circle {
    load_circle(env).unwrap_or_else(|_| Circle {
        id: env.current_contract_address(),
        token: env.current_contract_address(),
        name: String::from_str(env, ""),
        organizer: env.current_contract_address(),
        factory: env.current_contract_address(),
        contribution_amount: 0,
        max_members: 0,
        member_count: 0,
        payout_type: 0,
        total_rounds: 0,
        current_round: 0,
        status: 0,
        started_at: 0,
        created_at: 0,
        contribution_deadline_seconds: 0,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 0,
        grace_period_seconds: 0,
        max_strikes: 0,
        payout_bitmap: 0,
        total_payouts: 0,
        total_fees: 0,
        slug: String::from_str(env, ""),
    })
}

pub fn get_members(env: &Env) -> Vec<Member> {
    load_members(env).unwrap_or_else(|_| Vec::new(env))
}

pub fn get_contributions(
    env: &Env,
    member: &Address,
    page: u32,
    page_size: u32,
) -> Vec<Contribution> {
    let mut out = Vec::new(env);
    let start = page.saturating_mul(page_size.max(1));
    let mut count: u32 = 0;
    let total_rounds = load_circle(env).map(|circle| circle.total_rounds).unwrap_or(0);
    for round in 0..total_rounds {
        let contributions: Vec<Contribution> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundContributionRecords(round))
            .unwrap_or_else(|| Vec::new(env));
        for i in 0..contributions.len() {
            if let Some(c) = contributions.get(i) {
                if c.member != *member {
                    continue;
                }
                if count >= start && out.len() < page_size.max(1) {
                    out.push_back(c);
                }
                count = count.saturating_add(1);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Pause
// ---------------------------------------------------------------------------

pub fn pause_circle(env: &Env, admin: &Address) -> Result<(), CircleError> {
    is_admin(env, admin)?;
    pause::pause(env, admin).map_err(|_| CircleError::ContractPaused)
}

pub fn unpause_circle(env: &Env, admin: &Address) -> Result<(), CircleError> {
    is_admin(env, admin)?;
    pause::unpause(env, admin).map_err(|_| CircleError::ContractPaused)
}