use soroban_sdk::{Address, BytesN, Env, IntoVal, Map, Vec};
use crate::types::*;
use crate::payout;
use common::{math, pause};
use common::reentrancy::ReentrancyGuard;

pub fn init(env: &Env, admin: &Address, factory: &Address, config: &CircleConfig) -> Result<(), CircleError> {
    if config.max_members < 2 || config.contribution_amount <= 0 || config.total_rounds == 0 || config.payout_type > 3 {
        return Err(CircleError::InvalidAmount);
    }
    let circle = Circle {
        id: env.current_contract_address(),
        name: config.name.clone(),
        organizer: config.organizer.clone(),
        factory: factory.clone(),
        contribution_amount: config.contribution_amount,
        max_members: config.max_members,
        member_count: 0,
        payout_type: config.payout_type,
        total_rounds: config.total_rounds,
        current_round: 0,
        status: CircleStatus::Pending,
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
    env.storage().persistent().set(&DataKey::Members, &Vec::<Member>::new(env));
    env.storage().persistent().set(&DataKey::Contributions, &Map::<(Address, u32), Contribution>::new(env));
    env.storage().persistent().set(&DataKey::Payouts, &Vec::<PayoutRecipient>::new(env));
    env.storage().persistent().set(&DataKey::Bids, &Vec::<AuctionBid>::new(env));
    env.storage().persistent().set(&DataKey::Votes, &Vec::<VoteEntry>::new(env));
    Ok(())
}

pub fn join(env: &Env, member: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let mut circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.status == CircleStatus::Disputed || circle.status == CircleStatus::Completed || circle.status == CircleStatus::Cancelled {
        return Err(CircleError::NotActive);
    }
    let mut members: Vec<Member> = env.storage().persistent().get(&DataKey::Members).unwrap_or_else(|| Vec::new(env));
    for i in 0..members.len() {
        if members.get(i).ok_or(CircleError::VecAccessError)?.address == *member {
            return Err(CircleError::AlreadyMember);
        }
    }
    if members.len() as u32 >= circle.max_members {
        return Err(CircleError::CircleFull);
    }
    check_reputation_gates(env, &circle, member)?;
    let now = env.ledger().timestamp();
    let pos = members.len() as u32;
    members.push_back(Member {
        address: member.clone(),
        position: pos,
        joined_at: now,
        strikes: 0,
        status: MemberStatus::Active,
        exited_at: 0,
        total_contributions: 0,
        total_received: 0,
    });
    circle.member_count = circle.member_count.checked_add(1).ok_or(CircleError::CircleFull)?;
    if circle.member_count >= circle.max_members && circle.status == CircleStatus::Pending {
        circle.status = CircleStatus::Active;
        circle.started_at = now;
    }
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Members, &members);
    MemberJoined { member: member.clone(), position: pos }.publish(env);
    Ok(())
}

pub fn contribute(env: &Env, member: &Address, amount: i128, round: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.status != CircleStatus::Active {
        return Err(CircleError::NotActive);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    if amount != circle.contribution_amount {
        return Err(CircleError::ContributionMismatch);
    }
    let reg_opt: Option<Address> = env.storage().instance().get(&DataKey::ReputationRegistry);
    if let Some(reg) = reg_opt {
        let tier_max_contrib: i128 = env.invoke_contract(&reg, &soroban_sdk::Symbol::new(env, "calc_max_contrib"), soroban_sdk::vec![env, member.into_val(env)]);
        if amount > tier_max_contrib {
            return Err(CircleError::ContributionExceedsTier);
        }
    }
    let members: Vec<Member> = env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut found = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *member {
            if m.status != MemberStatus::Active {
                return Err(CircleError::InvalidMemberStatus);
            }
            found = true;
        }
    }
    if !found {
        return Err(CircleError::NotMember);
    }
    let mut contributions: Map<(Address, u32), Contribution> = env.storage().persistent().get(&DataKey::Contributions).unwrap_or_else(|| Map::new(env));
    let key = (member.clone(), round);
    if contributions.get(key.clone()).is_some() {
        return Err(CircleError::AlreadyContributed);
    }
    let now = env.ledger().timestamp();
    let deadline = circle.started_at.checked_add(circle.contribution_deadline_seconds).unwrap_or(u64::MAX);
    let on_time = now <= deadline;
    contributions.set(key, Contribution { member: member.clone(), round, amount, timestamp: now, on_time });
    env.storage().persistent().set(&DataKey::Contributions, &contributions);
    ContributionRecorded { member: member.clone(), round, amount, on_time }.publish(env);
    Ok(())
}

pub fn trigger_payout(env: &Env, caller: &Address, round: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    let mut circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(CircleError::NotInitialized)?;
    if caller != &circle.organizer && caller != &stored_admin {
        return Err(CircleError::Unauthorized);
    }
    if circle.status != CircleStatus::Active {
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
    let pool = math::safe_mul(circle.contribution_amount, circle.member_count as i128).map_err(|_| CircleError::InvalidAmount)?;
    let (net, fee) = math::apply_fee(pool, 0).map_err(|_| CircleError::InvalidAmount)?;
    let now = env.ledger().timestamp();
    let mut payouts: Vec<PayoutRecipient> = env.storage().persistent().get(&DataKey::Payouts).unwrap_or_else(|| Vec::new(env));
    payouts.push_back(PayoutRecipient {
        recipient: recipient.clone(),
        round,
        amount: net,
        fee,
        payout_type,
        timestamp: now,
    });
    circle.current_round = circle.current_round.checked_add(1).ok_or(CircleError::RoundNotCurrent)?;
    circle.total_payouts = math::safe_add(circle.total_payouts, net).map_err(|_| CircleError::InvalidAmount)?;
    circle.total_fees = math::safe_add(circle.total_fees, fee).map_err(|_| CircleError::InvalidAmount)?;
    if circle.current_round >= circle.total_rounds {
        circle.status = CircleStatus::Completed;
    }
    let mut members: Vec<Member> = env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    for i in 0..members.len() {
        let mut m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == recipient {
            m.total_received = math::safe_add(m.total_received, net).map_err(|_| CircleError::InvalidAmount)?;
            circle.payout_bitmap |= 1u128 << m.position;
            members.set(i, m);
        }
    }
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Payouts, &payouts);
    env.storage().persistent().set(&DataKey::Members, &members);
    if fee > 0 {
        if let Some(treasury) = env.storage().instance().get::<_, Address>(&DataKey::Treasury) {
            let _: () = env.invoke_contract(
                &treasury,
                &soroban_sdk::Symbol::new(env, "deposit_fee"),
                soroban_sdk::vec![
                    env,
                    env.current_contract_address().into_val(env),
                    fee.into_val(env),
                    circle.id.clone().into_val(env)
                ],
            );
        }
    }
    PayoutExecuted { recipient, round, amount: net, fee, payout_type }.publish(env);
    if circle.status == CircleStatus::Completed {
        CircleCompleted { total_payouts: circle.total_payouts, circle_id: circle.id.clone() }.publish(env);
    }
    Ok(())
}

pub fn auction_bid(env: &Env, bidder: &Address, discount_bips: u32, round: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    bidder.require_auth();
    let circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.payout_type != PAYOUT_AUCTION {
        return Err(CircleError::InvalidPayoutType);
    }
    if discount_bips > 10000 {
        return Err(CircleError::InvalidBid);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    let mut bids: Vec<AuctionBid> = env.storage().persistent().get(&DataKey::Bids).unwrap_or_else(|| Vec::new(env));
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
    AuctionBidPlaced { bidder: bidder.clone(), discount_bips, round }.publish(env);
    Ok(())
}

pub fn vote_payout(env: &Env, voter: &Address, vote_for: &Address, round: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    voter.require_auth();
    let circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.payout_type != PAYOUT_VOTE {
        return Err(CircleError::InvalidPayoutType);
    }
    if round != circle.current_round {
        return Err(CircleError::RoundNotCurrent);
    }
    let members: Vec<Member> = env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut is_member = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *voter {
            if m.status != MemberStatus::Active {
                return Err(CircleError::InvalidMemberStatus);
            }
            is_member = true;
        }
    }
    if !is_member {
        return Err(CircleError::NotMember);
    }
    let mut votes: Vec<VoteEntry> = env.storage().persistent().get(&DataKey::Votes).unwrap_or_else(|| Vec::new(env));
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
    VoteCast { voter: voter.clone(), vote_for: vote_for.clone(), round }.publish(env);
    Ok(())
}

pub fn exit(env: &Env, member: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let mut circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.status == CircleStatus::Completed || circle.status == CircleStatus::Cancelled {
        return Err(CircleError::NotActive);
    }
    let mut members: Vec<Member> = env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut penalty: i128 = 0;
    for i in 0..members.len() {
        let mut m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *member {
            if m.status != MemberStatus::Active {
                return Err(CircleError::InvalidMemberStatus);
            }
            let contributions: Map<(Address, u32), Contribution> = env.storage().persistent().get(&DataKey::Contributions).unwrap_or_else(|| Map::new(env));
            let mut ctotal: i128 = 0;
            for (_key, c) in contributions.iter() {
                if c.member == *member {
                    ctotal = math::safe_add(ctotal, c.amount).map_err(|_| CircleError::InvalidAmount)?;
                }
            }
            penalty = math::calculate_percentage(ctotal, 500).map_err(|_| CircleError::InvalidAmount)?;
            m.status = MemberStatus::Exited;
            m.exited_at = env.ledger().timestamp();
            members.set(i, m);
        }
    }
    if penalty > 0 {
        circle.total_fees = math::safe_add(circle.total_fees, penalty).map_err(|_| CircleError::InvalidAmount)?;
        env.storage().instance().set(&DataKey::Circle, &circle);
    }
    env.storage().persistent().set(&DataKey::Members, &members);
    MemberExited { member: member.clone(), penalty }.publish(env);
    Ok(())
}

pub fn report_late(env: &Env, reporter: &Address, late_member: &Address, round: u32) -> Result<(), CircleError> {
    reporter.require_auth();
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    let circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.status != CircleStatus::Active {
        return Err(CircleError::NotActive);
    }
    let contributions: Map<(Address, u32), Contribution> = env.storage().persistent().get(&DataKey::Contributions).unwrap_or_else(|| Map::new(env));
    let key = (late_member.clone(), round);
    let found = match contributions.get(key) {
        Some(c) => !c.on_time,
        None => false,
    };
    if !found {
        return Err(CircleError::NotMember);
    }
    let mut members: Vec<Member> = env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    for i in 0..members.len() {
        let mut m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *late_member {
            m.strikes = m.strikes.checked_add(1).ok_or(CircleError::MaxStrikesReached)?;
            if m.strikes >= circle.max_strikes {
                m.status = MemberStatus::Defaulted;
                MemberDefaulted { member: late_member.clone(), strikes: m.strikes }.publish(env);
            }
            members.set(i, m);
        }
    }
    env.storage().persistent().set(&DataKey::Members, &members);
    Ok(())
}

pub fn raise_dispute(env: &Env, member: &Address, evidence_hash: &BytesN<32>) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let mut circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.status == CircleStatus::Disputed {
        return Err(CircleError::DisputeAlreadyRaised);
    }
    if env.storage().persistent().get::<DataKey, DisputeEntry>(&DataKey::Dispute).is_some() {
        return Err(CircleError::DisputeAlreadyRaised);
    }
    circle.status = CircleStatus::Disputed;
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Dispute, &DisputeEntry {
        raised_by: member.clone(),
        evidence_hash: evidence_hash.clone(),
        raised_at: env.ledger().timestamp(),
        resolved_at: 0,
        resolution: 0,
        resolved_by: env.current_contract_address(),
    });
    DisputeRaised { member: member.clone(), evidence_hash: evidence_hash.clone() }.publish(env);
    Ok(())
}

pub fn resolve_dispute(env: &Env, admin: &Address, resolution: u32) -> Result<(), CircleError> {
    admin.require_auth();
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    let mut circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    let mut dispute: DisputeEntry = env.storage().persistent().get(&DataKey::Dispute).ok_or(CircleError::NoActiveDispute)?;
    if resolution > DisputeResolution::ForcePayout as u32 {
        return Err(CircleError::InvalidAmount);
    }
    match resolution {
        r if r == DisputeResolution::Dismiss as u32 => circle.status = CircleStatus::Active,
        r if r == DisputeResolution::ForcePayout as u32 => circle.status = CircleStatus::Active,
        _ => circle.status = CircleStatus::Cancelled,
    }
    dispute.resolved_at = env.ledger().timestamp();
    dispute.resolution = resolution;
    dispute.resolved_by = admin.clone();
    env.storage().instance().set(&DataKey::Circle, &circle);
    env.storage().persistent().set(&DataKey::Dispute, &dispute);
    if circle.status == CircleStatus::Cancelled {
        CircleCancelled { circle_id: circle.id.clone(), cancelled_by: admin.clone(), cancelled_at: env.ledger().timestamp() }.publish(env);
    }
    Ok(())
}

pub fn get_status(env: &Env) -> Circle {
    env.storage().instance().get(&DataKey::Circle).unwrap_or(Circle {
        id: env.current_contract_address(),
        name: soroban_sdk::String::from_str(env, ""),
        organizer: env.current_contract_address(),
        factory: env.current_contract_address(),
        contribution_amount: 0,
        max_members: 0,
        member_count: 0,
        payout_type: 0,
        total_rounds: 0,
        current_round: 0,
        status: CircleStatus::Cancelled,
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
    env.storage().persistent().get(&DataKey::Members).unwrap_or_else(|| Vec::new(env))
}

pub fn get_contributions(env: &Env, member: &Address) -> Vec<Contribution> {
    let all: Map<(Address, u32), Contribution> = env.storage().persistent().get(&DataKey::Contributions).unwrap_or_else(|| Map::new(env));
    let mut out = Vec::new(env);
    for (_key, c) in all.iter() {
        if c.member == *member {
            out.push_back(c);
        }
    }
    out
}

pub fn pause_circle(env: &Env, admin: &Address) -> Result<(), CircleError> {
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    pause::pause(env, admin).map_err(|_| CircleError::ContractPaused)
}

pub fn unpause_circle(env: &Env, admin: &Address) -> Result<(), CircleError> {
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(CircleError::NotInitialized)?;
    if admin != &s {
        return Err(CircleError::Unauthorized);
    }
    pause::unpause(env, admin).map_err(|_| CircleError::ContractPaused)
}

pub fn upgrade(env: &Env, admin: &Address, new_wasm_hash: &BytesN<32>) -> Result<(), CircleError> {
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(CircleError::NotInitialized)?;
    if admin != &stored_admin {
        return Err(CircleError::Unauthorized);
    }
    common::upgrade::upgrade_contract(env, admin, new_wasm_hash).map_err(|_| CircleError::Unauthorized)
}

pub fn register_referral(env: &Env, referrer: &Address, referred: &Address, bonus_pct: u32) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    referrer.require_auth();
    if referrer == referred {
        return Err(CircleError::SelfReferral);
    }
    let circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.status != CircleStatus::Active {
        return Err(CircleError::NotActive);
    }
    let members: Vec<Member> = env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut referrer_found = false;
    let mut referred_found = false;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == *referrer {
            referrer_found = true;
        }
        if m.address == *referred {
            referred_found = true;
        }
    }
    if !referrer_found || !referred_found {
        return Err(CircleError::NotMember);
    }
    let mut referrals: Vec<Referral> = env.storage().persistent().get(&DataKey::Referrals).unwrap_or_else(|| Vec::new(env));
    for i in 0..referrals.len() {
        let r = referrals.get(i).ok_or(CircleError::VecAccessError)?;
        if r.referred == *referred {
            return Err(CircleError::AlreadyReferred);
        }
    }
    if bonus_pct > 1000 {
        return Err(CircleError::InvalidBid);
    }
    referrals.push_back(Referral { referrer: referrer.clone(), referred: referred.clone(), bonus_pct, claimed: false });
    env.storage().persistent().set(&DataKey::Referrals, &referrals);
    ReferralRegistered { referrer: referrer.clone(), referred: referred.clone(), bonus_pct }.publish(env);
    Ok(())
}

pub fn claim_referral_bonus(env: &Env, referrer: &Address, _treasury: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    referrer.require_auth();
    let circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.status != CircleStatus::Active {
        return Err(CircleError::NotActive);
    }
    let mut referrals: Vec<Referral> = env.storage().persistent().get(&DataKey::Referrals).ok_or(CircleError::NotInitialized)?;
    let mut bonus_total: i128 = 0;
    for i in 0..referrals.len() {
        let mut r = referrals.get(i).ok_or(CircleError::VecAccessError)?;
        if r.referrer == *referrer && !r.claimed {
            let contributions: Map<(Address, u32), Contribution> = env.storage().persistent().get(&DataKey::Contributions).unwrap_or_else(|| Map::new(env));
            for (_key, c) in contributions.iter() {
                if c.member == r.referred {
                    let bonus = math::safe_mul(circle.contribution_amount, r.bonus_pct as i128).map_err(|_| CircleError::InvalidAmount)?;
                    let bonus_div = math::safe_div(bonus, 10000).map_err(|_| CircleError::InvalidAmount)?;
                    bonus_total = math::safe_add(bonus_total, bonus_div).map_err(|_| CircleError::InvalidAmount)?;
                }
            }
            r.claimed = true;
            referrals.set(i, r);
        }
    }
    if bonus_total <= 0 {
        return Err(CircleError::InvalidAmount);
    }
    env.storage().persistent().set(&DataKey::Referrals, &referrals);
    ReferralBonusPaid { referrer: referrer.clone(), amount: bonus_total }.publish(env);
    Ok(())
}

pub fn update_streak(env: &Env, member: &Address, round: u32) -> Result<(), CircleError> {
    // Only the member themselves can update their own streak — prevents arbitrary inflation.
    member.require_auth();
    let circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.status != CircleStatus::Active {
        return Err(CircleError::NotActive);
    }
    let mut streaks: Vec<Streak> = env.storage().persistent().get(&DataKey::Streaks).unwrap_or_else(|| Vec::new(env));
    let mut found = false;
    for i in 0..streaks.len() {
        let mut s = streaks.get(i).ok_or(CircleError::VecAccessError)?;
        if s.member == *member {
            s.last_round = round;
            s.current_streak = s.current_streak.checked_add(1).ok_or(CircleError::InvalidAmount)?;
            if s.current_streak > s.longest_streak {
                s.longest_streak = s.current_streak;
            }
            streaks.set(i, s.clone());
            found = true;
            StreakUpdated { member: member.clone(), current_streak: s.current_streak, longest_streak: s.longest_streak }.publish(env);
            break;
        }
    }
    if !found {
        streaks.push_back(Streak { member: member.clone(), current_streak: 1, longest_streak: 1, last_round: round });
        StreakUpdated { member: member.clone(), current_streak: 1, longest_streak: 1 }.publish(env);
    }
    env.storage().persistent().set(&DataKey::Streaks, &streaks);
    Ok(())
}

pub fn claim_streak_bonus(env: &Env, member: &Address, _treasury: &Address) -> Result<(), CircleError> {
    pause::when_not_paused(env).map_err(|_| CircleError::ContractPaused)?;
    let _guard = ReentrancyGuard::new(env).map_err(|_| CircleError::NotActive)?;
    member.require_auth();
    let circle: Circle = env.storage().instance().get(&DataKey::Circle).ok_or(CircleError::NotInitialized)?;
    if circle.status != CircleStatus::Active {
        return Err(CircleError::NotActive);
    }
    let streaks: Vec<Streak> = env.storage().persistent().get(&DataKey::Streaks).ok_or(CircleError::NotInitialized)?;
    let mut streak_val: u32 = 0;
    for i in 0..streaks.len() {
        let s = streaks.get(i).ok_or(CircleError::VecAccessError)?;
        if s.member == *member {
            streak_val = s.current_streak;
            break;
        }
    }
    if streak_val < 3 {
        return Err(CircleError::InvalidStreakThreshold);
    }
    let bonus = math::safe_mul(circle.contribution_amount, streak_val as i128).map_err(|_| CircleError::InvalidAmount)?;
    let bonus_div = math::safe_div(bonus, 100).map_err(|_| CircleError::InvalidAmount)?;
    StreakBonusPaid { member: member.clone(), amount: bonus_div, streak: streak_val }.publish(env);
    Ok(())
}

pub fn get_referrals(env: &Env) -> Vec<Referral> {
    env.storage().persistent().get(&DataKey::Referrals).unwrap_or_else(|| Vec::new(env))
}

pub fn get_streaks(env: &Env) -> Vec<Streak> {
    env.storage().persistent().get(&DataKey::Streaks).unwrap_or_else(|| Vec::new(env))
}

pub fn get_member_streak(env: &Env, member: &Address) -> Streak {
    let streaks: Vec<Streak> = env.storage().persistent().get(&DataKey::Streaks).unwrap_or_else(|| Vec::new(env));
    for i in 0..streaks.len() {
        if let Some(s) = streaks.get(i) {
            if s.member == *member {
                return s;
            }
        }
    }
    Streak { member: member.clone(), current_streak: 0, longest_streak: 0, last_round: 0 }
}

// ── #54/#55: reputation-registry wiring ──────────────────────────────────

pub fn set_reputation_registry(env: &Env, admin: &Address, registry: &Address) -> Result<(), CircleError> {
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(CircleError::NotInitialized)?;
    if admin != &stored_admin {
        return Err(CircleError::Unauthorized);
    }
    admin.require_auth();
    env.storage().instance().set(&DataKey::ReputationRegistry, registry);
    Ok(())
}

pub fn get_reputation_registry(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::ReputationRegistry)
}

fn check_reputation_gates(env: &Env, circle: &Circle, member: &Address) -> Result<(), CircleError> {
    let reg: Address = match env.storage().instance().get(&DataKey::ReputationRegistry) {
        Some(r) => r,
        None => return Ok(()),
    };
    let score: u32 = env.invoke_contract(&reg, &soroban_sdk::Symbol::new(env, "get_moi_score"), soroban_sdk::vec![env, member.into_val(env)]);
    if score < circle.min_moi_score {
        return Err(CircleError::InsufficientMoiScore);
    }
    let tier_max_size: u32 = env.invoke_contract(&reg, &soroban_sdk::Symbol::new(env, "calc_max_size"), soroban_sdk::vec![env, member.into_val(env)]);
    if circle.max_members > tier_max_size {
        return Err(CircleError::CircleSizeExceedsTier);
    }
    let tier_max_contrib: i128 = env.invoke_contract(&reg, &soroban_sdk::Symbol::new(env, "calc_max_contrib"), soroban_sdk::vec![env, member.into_val(env)]);
    if circle.contribution_amount > tier_max_contrib {
        return Err(CircleError::ContributionExceedsTier);
    }
    Ok(())
}

pub fn set_treasury(env: &Env, admin: &Address, treasury: &Address) -> Result<(), CircleError> {
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(CircleError::NotInitialized)?;
    if admin != &stored_admin { return Err(CircleError::Unauthorized); }
    admin.require_auth();
    env.storage().instance().set(&DataKey::Treasury, treasury);
    Ok(())
}

pub fn set_token(env: &Env, admin: &Address, token: &Address) -> Result<(), CircleError> {
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(CircleError::NotInitialized)?;
    if admin != &stored_admin { return Err(CircleError::Unauthorized); }
    admin.require_auth();
    env.storage().instance().set(&DataKey::Token, token);
    Ok(())
}

pub fn set_fee_bps(env: &Env, admin: &Address, fee_bps: u32) -> Result<(), CircleError> {
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(CircleError::NotInitialized)?;
    if admin != &stored_admin { return Err(CircleError::Unauthorized); }
    if fee_bps > 10_000 { return Err(CircleError::InvalidAmount); }
    admin.require_auth();
    env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
    Ok(())
}
// payout type constants (kept as u32 for CircleConfig compatibility)
pub const PAYOUT_RANDOM: u32 = 0;
pub const PAYOUT_FIXED: u32 = 1;
pub const PAYOUT_AUCTION: u32 = 2;
pub const PAYOUT_VOTE: u32 = 3;
