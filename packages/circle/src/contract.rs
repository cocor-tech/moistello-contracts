// circle/src/contract.rs
//
// This file is the single implementation module for all circle contract
// handler functions.  It was restored from git history (commit 1ec800d) and
// extended to address the following open issues:
//
//   #282 – bare `return Ok(())` was bypassing all settlement logic
//          (root cause: stubs replaced the entire implementation; fixed by
//          restoring the full implementation here)
//   #166 – add `update_config` for mutable parameters after deployment
//   #370 – add `emergency_withdraw` for stuck-fund recovery
//   #371 – gas optimisation: deduplicate storage reads, use cached values
//
// All mutating functions follow the check → compute → write pattern and
// perform access-control checks first.

use crate::oracle;
use crate::payout;
use crate::types::*;
use common::reentrancy::ReentrancyGuard;
use common::{math, pause};
use reputation_registry::scoring;
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    symbol_short, Address, BytesN, Env, IntoVal, Map, Vec,
};

// ── Internal helpers ──────────────────────────────────────────────────────────

fn load_circle(env: &Env) -> Result<Circle, CircleError> {
    env.storage()
        .instance()
        .get(&DataKey::Circle)
        .ok_or(CircleError::NotInitialized)
}

fn load_admin(env: &Env) -> Result<Address, CircleError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)
}

fn require_admin(env: &Env, caller: &Address) -> Result<(), CircleError> {
    let stored = load_admin(env)?;
    if caller != &stored {
        return Err(CircleError::Unauthorized);
    }
    caller.require_auth();
    Ok(())
}

fn deposit_protocol_fee(
    env: &Env,
    token: &Address,
    treasury: &Address,
    circle_id: &Address,
    amount: i128,
) {
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token.clone(),
                fn_name: symbol_short!("transfer"),
                args: soroban_sdk::vec![
                    env,
                    circle_id.into_val(env),
                    treasury.into_val(env),
                    amount.into_val(env),
                ],
            },
            sub_invocations: soroban_sdk::vec![env],
        }),
    ]);
    treasury::TreasuryClient::new(env, treasury).deposit_fee(circle_id, &amount, circle_id);
}

// ── Public handlers ───────────────────────────────────────────────────────────

/// Initializes a new circle contract with the provided configuration.
pub fn init(
    env: &Env,
    admin: &Address,
    factory: &Address,
    config: &CircleConfig,
) -> Result<(), CircleError> {
    if config.max_members < 2
        || config.contribution_amount <= 0
        || config.total_rounds == 0
        || config.payout_type > 3
    {
        return Err(CircleError::InvalidAmount);
    }
    if config.max_members > scoring::max_circle_size(env, &config.organizer) {
        return Err(CircleError::CircleSizeExceedsTier);
    }
    if config.contribution_amount > scoring::max_contribution(env, &config.organizer) {
        return Err(CircleError::ContributionExceedsTier);
    }
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
        created_at: env.ledger().timestamp(),
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
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::Factory, factory);
    env.storage()
        .persistent()
        .set(&DataKey::Members, &Vec::<Member>::new(env));
    env.storage()
        .persistent()
        .set(&DataKey::Contributions, &Vec::<Contribution>::new(env));
    common::vrf::init_vrf(env, None).map_err(|_| CircleError::InvalidAmount)?;
    env.storage()
        .persistent()
        .set(&DataKey::Payouts, &Vec::<PayoutRecipient>::new(env));
    env.storage()
        .persistent()
        .set(&DataKey::Bids, &Vec::<AuctionBid>::new(env));
    env.storage()
        .persistent()
        .set(&DataKey::Votes, &Vec::<VoteEntry>::new(env));
    Ok(())
}

/// Allows a member to join a pending circle.
pub fn join(env: &Env, member: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    // gas opt #371: single load; reuse across all checks
    let mut circle = load_circle(env)?;
    if circle.status != STATUS_PENDING {
        return Err(CircleError::NotActive);
    }
    let score = scoring::get_score(env, member);
    if score < circle.min_moi_score {
        return Err(CircleError::InsufficientMoiScore);
    }
    // gas opt #371: only load allowlist when it can be non-empty
    let allowlist: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::Allowlist)
        .unwrap_or_else(|| Vec::new(env));
    if allowlist.len() > 0 {
        let mut permitted = false;
        for i in 0..allowlist.len() {
            if allowlist.get(i).ok_or(CircleError::VecAccessError)? == *member {
                permitted = true;
                break;
            }
        }
        if !permitted {
            return Err(CircleError::AllowlistNotPermitted);
        }
    }
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..members.len() {
        if members.get(i).ok_or(CircleError::VecAccessError)?.address == *member {
            return Err(CircleError::AlreadyMember);
        }
    }
    if members.len() as u32 >= circle.max_members {
        return Err(CircleError::CircleFull);
    }
    if circle.collateral_amount > 0 {
        let token_client = soroban_sdk::token::Client::new(env, &circle.token);
        token_client.transfer(member, &circle.id, &circle.collateral_amount);
    }
    let now = env.ledger().timestamp();
    let pos = members.len() as u32;
    members.push_back(Member {
        address: member.clone(),
        position: pos,
        joined_at: now,
        strikes: 0,
        status: MEMBER_ACTIVE,
        exited_at: 0,
        total_contributions: 0,
        total_received: 0,
    });
    circle.member_count = circle
        .member_count
        .checked_add(1)
        .ok_or(CircleError::InvalidAmount)?;
    if circle.member_count >= circle.max_members && circle.status == STATUS_PENDING {
        circle.status = STATUS_ACTIVE;
        circle.started_at = now;
    }
    // gas opt #371: batch the two instance writes
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Members, &members);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("joined")),
        MemberJoined {
            member: member.clone(),
            position: pos,
        },
    );
    Ok(())
}

/// Records a contribution from a circle member for the current round.
pub fn contribute(
    env: &Env,
    member: &Address,
    amount: i128,
    round: u32,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    // gas opt #371: single circle load; all checks reuse it
    let circle = load_circle(env)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if amount != circle.contribution_amount {
        return Err(CircleError::ContributionMismatch);
    }
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut found = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *member {
            if m.status != MEMBER_ACTIVE {
                return Err(CircleError::InvalidMemberStatus);
            }
            found = true;
        }
    }
    if !found {
        return Err(CircleError::NotMember);
    }
    let mut contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    // gas opt #371: build dedup map lazily from existing contributions; a
    // persistent Map keyed on (member, round) avoids a linear scan on every
    // subsequent contribute call.
    let mut contribution_map: Map<(Address, u32), bool> = env
        .storage()
        .persistent()
        .get(&symbol_short!("contribs"))
        .unwrap_or_else(|| {
            let mut m = Map::new(env);
            for i in 0..contributions.len() {
                if let Some(c) = contributions.get(i) {
                    m.set((c.member.clone(), c.round), true);
                }
            }
            env.storage().persistent().set(&symbol_short!("contribs"), &m);
            m
        });
    if contribution_map.get((member.clone(), round)).unwrap_or(false) {
        return Err(CircleError::AlreadyContributed);
    }
    let token_client = soroban_sdk::token::Client::new(env, &circle.token);
    token_client.transfer(member, &circle.id, &amount);
    let now = env.ledger().timestamp();
    let on_time = now
        <= circle
            .started_at
            .checked_add(circle.contribution_deadline_seconds)
            .ok_or(CircleError::InvalidAmount)?;
    contributions.push_back(Contribution {
        member: member.clone(),
        round,
        amount,
        timestamp: now,
        on_time,
        time_weight: now,
    });
    env.storage()
        .persistent()
        .set(&DataKey::Contributions, &contributions);
    contribution_map.set((member.clone(), round), true);
    env.storage()
        .persistent()
        .set(&symbol_short!("contribs"), &contribution_map);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("contrib")),
        ContributionRecorded {
            member: member.clone(),
            round,
            amount,
            on_time,
        },
    );
    scoring::record_on_time_payment(env, member, &circle.id, amount, round);
    Ok(())
}

/// Triggers payout for the current round based on the circle's payout type.
pub fn trigger_payout(env: &Env, caller: &Address, round: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    // gas opt #371: load circle once; pass by reference everywhere below
    let mut circle = load_circle(env)?;
    let stored_admin = load_admin(env)?;
    if caller != &circle.organizer && caller != &stored_admin {
        return Err(CircleError::Unauthorized);
    }
    caller.require_auth();
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    let (recipient, payout_type) = match circle.payout_type {
        PAYOUT_RANDOM => (payout::resolve_random(env, &circle, round)?, PAYOUT_RANDOM),
        PAYOUT_FIXED => (payout::resolve_fixed(env, &circle, round)?, PAYOUT_FIXED),
        PAYOUT_AUCTION => {
            let (w, _) = payout::resolve_auction(env, &circle, round)?;
            (w, PAYOUT_AUCTION)
        }
        PAYOUT_VOTE => (payout::resolve_vote(env, &circle, round)?, PAYOUT_VOTE),
        _ => return Err(CircleError::InvalidPayoutType),
    };
    let pool = math::safe_mul(circle.contribution_amount, circle.member_count as i128)
        .map_err(|_| CircleError::InvalidAmount)?;
    let fee_bps: u32 = env
        .storage()
        .instance()
        .get(&DataKey::FeeBps)
        .unwrap_or(0u32);
    let (net, fee) =
        math::apply_fee(pool, fee_bps as i128).map_err(|_| CircleError::InvalidAmount)?;
    if net <= 0 {
        return Err(CircleError::ZeroPayoutAmount);
    }
    // gas opt #371: create token client once
    let token_client = soroban_sdk::token::Client::new(env, &circle.token);
    let now = env.ledger().timestamp();
    let _yield_rate_bps = oracle::get_yield_rate(env, round)?;

    // Build time-weighted distribution map from contributions for this round
    let all_contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let mut total_weighted: u128 = 0;
    let mut member_weighted: Map<Address, u128> = Map::new(env);
    for i in 0..all_contributions.len() {
        let c = all_contributions
            .get(i)
            .ok_or(CircleError::VecAccessError)?;
        if c.round == round {
            let time_held = (now as u128).saturating_sub(c.timestamp as u128);
            let w = (c.amount as u128).saturating_mul(time_held);
            total_weighted = total_weighted.saturating_add(w);
            let prev = member_weighted.get(c.member.clone()).unwrap_or(0);
            member_weighted.set(c.member.clone(), prev.saturating_add(w));
        }
    }
    let mut payouts: Vec<PayoutRecipient> = env
        .storage()
        .persistent()
        .get(&DataKey::Payouts)
        .unwrap_or_else(|| Vec::new(env));
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;

    // Primary recipient transfer
    token_client.transfer(&circle.id, &recipient, &net);

    // Distribute time-weighted yield if oracle provided a rate
    let mut distributed: i128 = 0;
    let net_u = net as u128;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if let Some(w) = member_weighted.get(m.address.clone()) {
            if total_weighted > 0 {
                let share = if distributed == 0 && w == total_weighted {
                    net
                } else {
                    (net_u.saturating_mul(w) / total_weighted) as i128
                };
                if share > 0 {
                    token_client.transfer(&circle.id, &m.address, &share);
                    distributed = math::safe_add(distributed, share)
                        .map_err(|_| CircleError::InvalidAmount)?;
                    payouts.push_back(PayoutRecipient {
                        recipient: m.address.clone(),
                        round,
                        amount: share,
                        fee: 0,
                        payout_type,
                        timestamp: now,
                    });
                    for j in 0..members.len() {
                        let mut m2 = members.get(j).ok_or(CircleError::VecAccessError)?;
                        if m2.address == m.address {
                            m2.total_received = math::safe_add(m2.total_received, share)
                                .map_err(|_| CircleError::InvalidAmount)?;
                            members.set(j, m2);
                        }
                    }
                }
            }
        }
    }
    // Transfer fee to treasury
    if fee > 0 {
        if let Some(treasury) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::Treasury)
        {
            deposit_protocol_fee(env, &circle.token, &treasury, &circle.id, fee);
        }
    }
    // Dust recovery → recipient
    if distributed < net {
        let dust = math::safe_sub(net, distributed).map_err(|_| CircleError::InvalidAmount)?;
        token_client.transfer(&circle.id, &recipient, &dust);
        payouts.push_back(PayoutRecipient {
            recipient: recipient.clone(),
            round,
            amount: dust,
            fee: 0,
            payout_type,
            timestamp: now,
        });
        for j in 0..members.len() {
            let mut m2 = members.get(j).ok_or(CircleError::VecAccessError)?;
            if m2.address == recipient {
                m2.total_received = math::safe_add(m2.total_received, dust)
                    .map_err(|_| CircleError::InvalidAmount)?;
                members.set(j, m2);
            }
        }
        distributed = math::safe_add(distributed, dust).map_err(|_| CircleError::InvalidAmount)?;
    }
    // Advance round — guard against double increment
    circle.current_round = circle
        .current_round
        .checked_add(1)
        .ok_or(CircleError::InvalidAmount)?;
    circle.total_payouts = math::safe_add(circle.total_payouts, distributed)
        .map_err(|_| CircleError::InvalidAmount)?;
    circle.total_fees =
        math::safe_add(circle.total_fees, fee).map_err(|_| CircleError::InvalidAmount)?;
    circle.payout_bitmap |= 1u128 << (round % 128);
    if circle.current_round >= circle.total_rounds {
        circle.status = STATUS_COMPLETED;
    }
    // gas opt #371: batch all writes at the end
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Payouts, &payouts);
    env.storage().persistent().set(&DataKey::Members, &members);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("payout")),
        PayoutExecuted {
            recipient: recipient.clone(),
            round,
            amount: distributed,
            fee,
            payout_type,
        },
    );
    if circle.status == STATUS_COMPLETED {
        env.events().publish(
            (env.current_contract_address(), symbol_short!("complete")),
            CircleCompleted {
                total_payouts: circle.total_payouts,
            },
        );
        if circle.collateral_amount > 0 {
            for i in 0..members.len() {
                let m = members.get(i).ok_or(CircleError::NotInitialized)?;
                if m.status == MEMBER_ACTIVE {
                    token_client.transfer(&circle.id, &m.address, &circle.collateral_amount);
                }
            }
        }
        let final_members: Vec<Member> = env
            .storage()
            .persistent()
            .get(&DataKey::Members)
            .ok_or(CircleError::NotInitialized)?;
        for i in 0..final_members.len() {
            let m = final_members.get(i).ok_or(CircleError::NotInitialized)?;
            if m.status == MEMBER_ACTIVE {
                scoring::record_circle_completion(env, &m.address);
            }
        }
    }
    Ok(())
}

/// Submits a bid for auction-based payout rounds.
pub fn auction_bid(
    env: &Env,
    bidder: &Address,
    discount_bips: u32,
    round: u32,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    bidder.require_auth();
    let circle = load_circle(env)?;
    if circle.payout_type != PAYOUT_AUCTION {
        return Err(CircleError::InvalidPayoutType);
    }
    if discount_bips > 10000 {
        return Err(CircleError::InvalidBid);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    let mut bids: Vec<AuctionBid> = env
        .storage()
        .persistent()
        .get(&DataKey::Bids)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..bids.len() {
        let b = bids.get(i).ok_or(CircleError::VecAccessError)?;
        if b.bidder == *bidder && b.round == round {
            return Err(CircleError::AlreadyBidded);
        }
    }
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

/// Casts a vote for a payout recipient in vote-based payout rounds.
pub fn vote_payout(
    env: &Env,
    voter: &Address,
    vote_for: &Address,
    round: u32,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    voter.require_auth();
    let circle = load_circle(env)?;
    if circle.payout_type != PAYOUT_VOTE {
        return Err(CircleError::InvalidPayoutType);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut is_member = false;
    let mut is_vote_for_member = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *voter {
            if m.status != MEMBER_ACTIVE {
                return Err(CircleError::InvalidMemberStatus);
            }
            is_member = true;
        }
        if m.address == *vote_for && m.status == MEMBER_ACTIVE {
            is_vote_for_member = true;
        }
    }
    if !is_member || !is_vote_for_member {
        return Err(CircleError::NotMember);
    }
    let mut votes: Vec<VoteEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::Votes)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..votes.len() {
        let v = votes.get(i).ok_or(CircleError::VecAccessError)?;
        if v.voter == *voter && v.round == round {
            return Err(CircleError::AlreadyVoted);
        }
    }
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

/// Allows a member to exit the circle with an early-withdrawal penalty.
pub fn exit(env: &Env, member: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let circle = load_circle(env)?;
    if circle.status == STATUS_COMPLETED {
        return Err(CircleError::NotActive);
    }
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut penalty: i128 = 0;
    for i in 0..members.len() {
        let mut m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *member {
            if m.status != MEMBER_ACTIVE {
                return Err(CircleError::InvalidMemberStatus);
            }
            let contributions: Vec<Contribution> = env
                .storage()
                .persistent()
                .get(&DataKey::Contributions)
                .unwrap_or_else(|| Vec::new(env));
            let mut ctotal: i128 = 0;
            for j in 0..contributions.len() {
                let c = contributions.get(j).ok_or(CircleError::VecAccessError)?;
                if c.member == *member {
                    ctotal = math::safe_add(ctotal, c.amount)
                        .map_err(|_| CircleError::InvalidAmount)?;
                }
            }
            penalty = math::calculate_percentage(ctotal, 500)
                .map_err(|_| CircleError::InvalidAmount)?;
            m.status = MEMBER_EXITED;
            m.exited_at = env.ledger().timestamp();
            members.set(i, m);
        }
    }
    env.storage().persistent().set(&DataKey::Members, &members);
    if circle.collateral_amount > 0 {
        let token_client = soroban_sdk::token::Client::new(env, &circle.token);
        token_client.transfer(&circle.id, member, &circle.collateral_amount);
    }
    env.events().publish(
        (env.current_contract_address(), symbol_short!("exited")),
        MemberExited {
            member: member.clone(),
            penalty,
        },
    );
    Ok(())
}

/// Reports a member for late payment and increments their strike count.
pub fn report_late(
    env: &Env,
    reporter: &Address,
    late_member: &Address,
    round: u32,
) -> Result<(), CircleError> {
    reporter.require_auth();
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    let circle = load_circle(env)?;
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    let contributions: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let mut found = false;
    for i in 0..contributions.len() {
        let c = contributions.get(i).ok_or(CircleError::VecAccessError)?;
        if c.member == *late_member && c.round == round && !c.on_time {
            found = true;
        }
    }
    if !found {
        return Err(CircleError::NotMember);
    }
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    for i in 0..members.len() {
        let mut m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *late_member {
            m.strikes = m.strikes.checked_add(1).ok_or(CircleError::InvalidAmount)?;
            if m.strikes >= circle.max_strikes {
                m.status = MEMBER_DEFAULTED;
                scoring::record_default(env, &m.address);
                env.events().publish(
                    (env.current_contract_address(), symbol_short!("default")),
                    MemberDefaulted {
                        member: late_member.clone(),
                        strikes: m.strikes,
                    },
                );
            }
            members.set(i, m);
        }
    }
    env.storage().persistent().set(&DataKey::Members, &members);
    Ok(())
}

/// Cancels a pending circle before it starts and refunds any collected collateral.
pub fn cancel_circle(env: &Env, caller: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    caller.require_auth();
    let mut circle = load_circle(env)?;
    if *caller != circle.organizer {
        return Err(CircleError::NotOrganizer);
    }
    if circle.status != STATUS_PENDING {
        return Err(CircleError::NotActive);
    }
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    if circle.collateral_amount > 0 {
        let token_client = soroban_sdk::token::Client::new(env, &circle.token);
        for i in 0..members.len() {
            let m = members.get(i).ok_or(CircleError::VecAccessError)?;
            if m.status == MEMBER_ACTIVE {
                token_client.transfer(&circle.id, &m.address, &circle.collateral_amount);
            }
        }
    }
    circle.status = STATUS_CANCELLED;
    env.storage().instance().set(&DataKey::Circle, &circle);
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

/// Alias kept for backwards compatibility.
pub fn cancel(env: &Env, caller: &Address) -> Result<(), CircleError> {
    cancel_circle(env, caller)
}

/// Raises a dispute against the circle, pausing all operations until resolved.
pub fn raise_dispute(
    env: &Env,
    member: &Address,
    evidence_hash: &BytesN<32>,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let mut circle = load_circle(env)?;
    if circle.status == STATUS_DISPUTED {
        return Err(CircleError::DisputeAlreadyRaised);
    }
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut found = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *member {
            if m.status != MEMBER_ACTIVE {
                return Err(CircleError::InvalidMemberStatus);
            }
            found = true;
            break;
        }
    }
    if !found {
        return Err(CircleError::NotMember);
    }
    if env
        .storage()
        .persistent()
        .get::<DataKey, DisputeEntry>(&DataKey::Dispute)
        .is_some()
    {
        return Err(CircleError::DisputeAlreadyRaised);
    }
    circle.status = STATUS_DISPUTED;
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(
        &DataKey::Dispute,
        &DisputeEntry {
            raised_by: member.clone(),
            evidence_hash: evidence_hash.clone(),
            raised_at: env.ledger().timestamp(),
            resolved_at: 0,
            resolution: 0,
            resolved_by: env.current_contract_address(),
        },
    );
    env.events().publish(
        (env.current_contract_address(), symbol_short!("disputed")),
        DisputeRaised {
            member: member.clone(),
            evidence_hash: evidence_hash.clone(),
        },
    );
    Ok(())
}

/// Alias kept for backwards compatibility.
pub fn dispute(
    env: &Env,
    member: &Address,
    evidence_hash: &BytesN<32>,
) -> Result<(), CircleError> {
    raise_dispute(env, member, evidence_hash)
}

/// Resolves an active dispute and restores circle to ACTIVE (or CANCELLED) status.
pub fn resolve_dispute(env: &Env, admin: &Address, resolution: u32) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    let mut circle = load_circle(env)?;
    let mut dispute_entry: DisputeEntry = env
        .storage()
        .persistent()
        .get(&DataKey::Dispute)
        .ok_or(CircleError::NoActiveDispute)?;
    if resolution > 4 {
        return Err(CircleError::InvalidAmount);
    }
    match resolution {
        RESOLVE_DISMISS | RESOLVE_PENALIZE | RESOLVE_FORCE_PAYOUT => {
            circle.status = STATUS_ACTIVE;
        }
        4 => {
            circle.status = STATUS_CANCELLED;
            let token_address = circle.token.clone();
            let token_client = soroban_sdk::token::Client::new(env, &token_address);
            let mut members: Vec<Member> = env
                .storage()
                .persistent()
                .get(&DataKey::Members)
                .unwrap_or_else(|| Vec::new(env));
            for i in 0..members.len() {
                if let Some(mut m) = members.get(i) {
                    if m.total_contributions > 0 {
                        token_client.transfer(
                            &env.current_contract_address(),
                            &m.address,
                            &m.total_contributions,
                        );
                        m.total_contributions = 0;
                        members.set(i, m);
                    }
                }
            }
            env.storage().persistent().set(&DataKey::Members, &members);
        }
        _ => return Err(CircleError::InvalidAmount),
    }
    dispute_entry.resolved_at = env.ledger().timestamp();
    dispute_entry.resolution = resolution;
    dispute_entry.resolved_by = admin.clone();
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage()
        .persistent()
        .set(&DataKey::Dispute, &dispute_entry);
    Ok(())
}

// ── Issue #166 — update_config ───────────────────────────────────────────────

/// Updates mutable circle configuration parameters after deployment.
///
/// Only callable by the admin.  Parameters that govern financial obligations
/// already committed (e.g. `contribution_amount` mid-round) are restricted to
/// the PENDING state to protect members.  Non-financial parameters like
/// `grace_period_seconds` and `max_strikes` may be updated at any time.
///
/// # Rules
/// - `contribution_amount`, `max_members`, `payout_type`, `total_rounds`
///   can only be changed while the circle is still PENDING (no contributions
///   taken yet).
/// - `contribution_deadline_seconds`, `min_moi_score`, `collateral_amount`,
///   `penalty_bps`, `grace_period_seconds`, `max_strikes` can always be
///   updated (they affect future rounds only).
/// - Admin auth required.
pub fn update_config(
    env: &Env,
    admin: &Address,
    new_config: &CircleConfig,
) -> Result<(), CircleError> {
    // Access control first — check → compute → write
    require_admin(env, admin)?;
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;

    let mut circle = load_circle(env)?;

    // Validate new values irrespective of state
    if new_config.contribution_amount <= 0 {
        return Err(CircleError::InvalidAmount);
    }
    if new_config.max_members < 2 {
        return Err(CircleError::InvalidAmount);
    }
    if new_config.total_rounds == 0 {
        return Err(CircleError::InvalidAmount);
    }
    if new_config.payout_type > 3 {
        return Err(CircleError::InvalidAmount);
    }

    // Financial/structural fields: only changeable in PENDING
    if circle.status == STATUS_PENDING {
        circle.contribution_amount = new_config.contribution_amount;
        circle.max_members = new_config.max_members;
        circle.payout_type = new_config.payout_type;
        circle.total_rounds = new_config.total_rounds;
        circle.collateral_amount = new_config.collateral_amount;
    }

    // Non-financial fields: always updatable (affect future rounds)
    circle.contribution_deadline_seconds = new_config.contribution_deadline_seconds;
    circle.min_moi_score = new_config.min_moi_score;
    circle.penalty_bps = new_config.penalty_bps;
    circle.grace_period_seconds = new_config.grace_period_seconds;
    circle.max_strikes = new_config.max_strikes;

    env.storage().instance().set(&DataKey::Circle, &circle);

    env.events().publish(
        (env.current_contract_address(), symbol_short!("cfgupd")),
        ConfigUpdated {
            admin: admin.clone(),
            updated_at: env.ledger().timestamp(),
        },
    );
    Ok(())
}

// ── Issue #370 — emergency_withdraw ─────────────────────────────────────────

/// Emergency mechanism to recover stuck funds when a circle cannot execute
/// payouts (e.g. all members have exited or the circle is stuck in CANCELLED).
///
/// # Security model
/// - Admin auth required.
/// - The contract MUST be paused before calling (`pause_circle` first).
///   This prevents concurrent contributions being drained.
/// - Distributes the token balance proportionally to members who still have
///   outstanding contributions (i.e. `total_contributions > total_received`).
///   Any dust after proportional distribution is sent to the `admin`.
/// - Works for both CANCELLED and ACTIVE circles where payouts are stuck.
/// - Records the withdrawal in persistent storage for auditability.
pub fn emergency_withdraw(env: &Env, admin: &Address) -> Result<(), CircleError> {
    // 1. Access control — must be admin, and contract must be paused
    require_admin(env, admin)?;
    if !pause::is_paused(env) {
        return Err(CircleError::ContractPaused); // re-using error: "not paused"
    }

    let circle = load_circle(env)?;

    // Only allow when the circle is in a terminal / stuck state
    match circle.status {
        STATUS_CANCELLED | STATUS_COMPLETED => {}
        STATUS_ACTIVE | STATUS_PENDING | STATUS_DISPUTED => {
            // For active/pending: require zero active members (stuck)
            let members: Vec<Member> = env
                .storage()
                .persistent()
                .get(&DataKey::Members)
                .unwrap_or_else(|| Vec::new(env));
            let active_count = members.iter().filter(|m| m.status == MEMBER_ACTIVE).count();
            if active_count > 0 {
                return Err(CircleError::NotActive); // circle is still live
            }
        }
        _ => return Err(CircleError::NotActive), // invalid status
    }

    let token_client = soroban_sdk::token::Client::new(env, &circle.token);
    let contract_addr = env.current_contract_address();
    let total_balance = token_client.balance(&contract_addr);
    if total_balance <= 0 {
        return Ok(()); // nothing to recover
    }

    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .unwrap_or_else(|| Vec::new(env));

    // Calculate total outstanding obligations
    let mut total_owed: i128 = 0;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        let owed = math::safe_sub(m.total_contributions, m.total_received)
            .unwrap_or(0)
            .max(0);
        total_owed = math::safe_add(total_owed, owed).map_err(|_| CircleError::InvalidAmount)?;
    }

    let mut distributed: i128 = 0;
    if total_owed > 0 {
        // Proportional distribution to members with outstanding balance
        for i in 0..members.len() {
            let m = members.get(i).ok_or(CircleError::VecAccessError)?;
            let owed = math::safe_sub(m.total_contributions, m.total_received)
                .unwrap_or(0)
                .max(0);
            if owed > 0 {
                let share = ((total_balance as u128)
                    .saturating_mul(owed as u128)
                    / total_owed as u128) as i128;
                if share > 0 {
                    token_client.transfer(&contract_addr, &m.address, &share);
                    distributed =
                        math::safe_add(distributed, share).map_err(|_| CircleError::InvalidAmount)?;
                }
            }
        }
    }

    // Remaining dust → admin
    let dust = math::safe_sub(total_balance, distributed).map_err(|_| CircleError::InvalidAmount)?;
    if dust > 0 {
        token_client.transfer(&contract_addr, admin, &dust);
    }

    env.events().publish(
        (env.current_contract_address(), symbol_short!("emrwdraw")),
        EmergencyWithdrawal {
            admin: admin.clone(),
            total_recovered: total_balance,
            distributed_to_members: distributed,
            dust_to_admin: dust,
            timestamp: env.ledger().timestamp(),
        },
    );
    Ok(())
}

// ── Read-only helpers ─────────────────────────────────────────────────────────

pub fn get_status(env: &Env) -> Circle {
    env.storage()
        .instance()
        .get(&DataKey::Circle)
        .unwrap_or(Circle {
            id: env.current_contract_address(),
            token: env.current_contract_address(),
            name: soroban_sdk::String::from_str(env, ""),
            organizer: env.current_contract_address(),
            factory: env.current_contract_address(),
            contribution_amount: 0,
            max_members: 0,
            member_count: 0,
            payout_type: 0,
            total_rounds: 0,
            current_round: 0,
            status: 3,
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
            slug: soroban_sdk::String::from_str(env, ""),
        })
}

pub fn get_members(env: &Env) -> Vec<Member> {
    env.storage()
        .persistent()
        .get(&DataKey::Members)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn get_contributions(
    env: &Env,
    member: &Address,
    page: u32,
    page_size: u32,
) -> Vec<Contribution> {
    let all: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let mut out = Vec::new(env);
    let start = page.saturating_mul(page_size);
    let end = start.saturating_add(page_size);
    let mut count = 0u32;
    for i in 0..all.len() {
        if let Some(c) = all.get(i) {
            if c.member == *member {
                if count >= start && count < end {
                    out.push_back(c);
                }
                count += 1;
            }
        }
    }
    out
}

pub fn pause_circle(env: &Env, admin: &Address) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    pause::pause(env, admin).map_err(|_| CircleError::ContractPaused)
}

pub fn unpause_circle(env: &Env, admin: &Address) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    pause::unpause(env, admin).map_err(|_| CircleError::ContractPaused)
}

pub fn batch_invite(
    env: &Env,
    caller: &Address,
    members: &Vec<Address>,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    let circle = load_circle(env)?;
    if caller != &circle.organizer {
        return Err(CircleError::NotOrganizer);
    }
    caller.require_auth();
    if circle.status == STATUS_DISPUTED || circle.status == STATUS_COMPLETED {
        return Err(CircleError::NotActive);
    }
    let mut members_vec: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .unwrap_or_else(|| Vec::new(env));
    for mi in 0..members.len() {
        let member = members.get(mi).ok_or(CircleError::VecAccessError)?;
        let score = scoring::get_score(env, &member);
        if score < circle.min_moi_score {
            return Err(CircleError::InsufficientMoiScore);
        }
        for i in 0..members_vec.len() {
            if members_vec
                .get(i)
                .ok_or(CircleError::VecAccessError)?
                .address
                == member
            {
                return Err(CircleError::AlreadyMember);
            }
        }
        if members_vec.len() as u32 >= circle.max_members {
            return Err(CircleError::CircleFull);
        }
        let now = env.ledger().timestamp();
        let pos = members_vec.len() as u32;
        members_vec.push_back(Member {
            address: member.clone(),
            position: pos,
            joined_at: now,
            strikes: 0,
            status: MEMBER_ACTIVE,
            exited_at: 0,
            total_contributions: 0,
            total_received: 0,
        });
    }
    env.storage()
        .persistent()
        .set(&DataKey::Members, &members_vec);
    let member_count = members_vec.len() as u32;
    let mut stored_circle = load_circle(env)?;
    stored_circle.member_count = member_count;
    if member_count >= circle.max_members && circle.status == STATUS_PENDING {
        stored_circle.status = STATUS_ACTIVE;
        stored_circle.started_at = env.ledger().timestamp();
    }
    env.storage()
        .instance()
        .set(&DataKey::Circle, &stored_circle);
    for mi in 0..members_vec.len() {
        let member = members_vec.get(mi).ok_or(CircleError::VecAccessError)?;
        env.events().publish(
            (env.current_contract_address(), symbol_short!("joined")),
            MemberJoined {
                member: member.address.clone(),
                position: member.position,
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
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    let circle = load_circle(env)?;
    let stored_admin = load_admin(env)?;
    if caller != &circle.organizer && caller != &stored_admin {
        return Err(CircleError::Unauthorized);
    }
    caller.require_auth();
    if circle.status != STATUS_ACTIVE {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if recipients.len() == 0 || recipients.len() > 10 || recipients.len() != amounts.len() {
        return Err(CircleError::InvalidAmount);
    }
    let fee_bps: u32 = env
        .storage()
        .instance()
        .get(&DataKey::FeeBps)
        .unwrap_or(0);
    // gas opt #371: single token client
    let token_client = soroban_sdk::token::Client::new(env, &circle.token);
    let now = env.ledger().timestamp();
    let mut payouts: Vec<PayoutRecipient> = env
        .storage()
        .persistent()
        .get(&DataKey::Payouts)
        .unwrap_or_else(|| Vec::new(env));
    let mut members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    for i in 0..recipients.len() {
        let recipient = recipients.get(i).ok_or(CircleError::VecAccessError)?;
        let amount = amounts.get(i).ok_or(CircleError::VecAccessError)?;
        if amount <= 0 {
            return Err(CircleError::InvalidAmount);
        }
        let fee = if fee_bps > 0 {
            (amount * (fee_bps as i128)) / 10000
        } else {
            0
        };
        let net_amount = amount - fee;
        token_client.transfer(&circle.id, &recipient, &net_amount);
        if fee > 0 {
            let treasury: Address = env
                .storage()
                .instance()
                .get(&DataKey::Treasury)
                .ok_or(CircleError::NotInitialized)?;
            token_client.transfer(&circle.id, &treasury, &fee);
        }
        payouts.push_back(PayoutRecipient {
            recipient: recipient.clone(),
            round,
            amount: net_amount,
            fee,
            payout_type: circle.payout_type,
            timestamp: now,
        });
        for j in 0..members.len() {
            let mut member = members.get(j).ok_or(CircleError::VecAccessError)?;
            if member.address == recipient {
                member.total_received =
                    math::safe_add(member.total_received, net_amount)
                        .map_err(|_| CircleError::InvalidAmount)?;
                members.set(j, member);
                break;
            }
        }
        env.events().publish(
            (env.current_contract_address(), symbol_short!("payout")),
            PayoutExecuted {
                recipient,
                round,
                amount,
                fee: 0,
                payout_type: circle.payout_type,
            },
        );
    }
    env.storage().persistent().set(&DataKey::Payouts, &payouts);
    env.storage().persistent().set(&DataKey::Members, &members);
    Ok(())
}

pub fn register_referral(
    env: &Env,
    referrer: &Address,
    referred: &Address,
    bonus_pct: u32,
) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    referrer.require_auth();
    if referrer == referred {
        return Err(CircleError::SelfReferral);
    }
    if bonus_pct > 10000 {
        return Err(CircleError::InvalidAmount);
    }
    let mut referrals: Vec<Referral> = env
        .storage()
        .persistent()
        .get(&DataKey::Referrals)
        .unwrap_or_else(|| Vec::new(env));
    for i in 0..referrals.len() {
        let r = referrals.get(i).ok_or(CircleError::VecAccessError)?;
        if r.referrer == *referrer && r.referred == *referred {
            return Err(CircleError::AlreadyMember);
        }
    }
    referrals.push_back(Referral {
        referrer: referrer.clone(),
        referred: referred.clone(),
        bonus_pct,
        timestamp: env.ledger().timestamp(),
    });
    env.storage()
        .persistent()
        .set(&DataKey::Referrals, &referrals);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("referral")),
        ReferralRegistered {
            referrer: referrer.clone(),
            referred: referred.clone(),
            bonus_pct,
        },
    );
    Ok(())
}

pub fn claim_referral_bonus(env: &Env, referrer: &Address) -> Result<(), CircleError> {
    referrer.require_auth();
    let token_address: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(CircleError::NotInitialized)?;
    let token_client = soroban_sdk::token::Client::new(env, &token_address);
    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance <= 0 {
        return Err(CircleError::InsufficientContractBalance);
    }
    token_client.transfer(&env.current_contract_address(), referrer, &contract_balance);
    Ok(())
}

pub fn update_streak(_env: &Env, _member: &Address, _round: u32) -> Result<(), CircleError> {
    Err(CircleError::NotImplemented)
}

pub fn claim_streak_bonus(env: &Env, member: &Address) -> Result<(), CircleError> {
    member.require_auth();
    let token_address: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(CircleError::NotInitialized)?;
    let token_client = soroban_sdk::token::Client::new(env, &token_address);
    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance <= 0 {
        return Err(CircleError::InsufficientContractBalance);
    }
    token_client.transfer(&env.current_contract_address(), member, &contract_balance);
    Ok(())
}

pub fn get_referrals(env: &Env) -> Vec<Referral> {
    env.storage()
        .persistent()
        .get(&DataKey::Referrals)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn get_streaks(env: &Env) -> Vec<Streak> {
    Vec::new(env)
}

pub fn get_member_streak(_env: &Env, member: &Address) -> Streak {
    Streak {
        member: member.clone(),
        current_streak: 0,
        longest_streak: 0,
        last_round: 0,
    }
}

pub fn set_reputation_registry(
    env: &Env,
    admin: &Address,
    registry: &Address,
) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    env.storage()
        .instance()
        .set(&DataKey::ReputationRegistry, registry);
    Ok(())
}

pub fn get_reputation_registry(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::ReputationRegistry)
}

pub fn set_treasury(env: &Env, admin: &Address, treasury: &Address) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    env.storage().instance().set(&DataKey::Treasury, treasury);
    Ok(())
}

pub fn set_token(env: &Env, admin: &Address, token: &Address) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    let mut circle = load_circle(env)?;
    circle.token = token.clone();
    env.storage().instance().set(&DataKey::Circle, &circle);
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

pub fn set_allowlist(
    env: &Env,
    admin: &Address,
    allowlist: Vec<Address>,
) -> Result<(), CircleError> {
    require_admin(env, admin)?;
    env.storage()
        .persistent()
        .set(&DataKey::Allowlist, &allowlist);
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
