use crate::types::*;
use common::{math, vrf};
use soroban_sdk::{Address, Env, Map, Vec};

pub fn resolve_random(env: &Env, circle: &Circle, _round: u32) -> Result<Address, CircleError> {
    let positions =
        vrf::shuffle_positions(env, circle.max_members).map_err(|_| CircleError::InvalidAmount)?;
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut pos_to_addr: Map<u32, Address> = Map::new(env);
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status == MEMBER_ACTIVE {
            pos_to_addr.set(m.position, m.address.clone());
        }
    }
    for i in 0..positions.len() {
        let pos = positions.get(i).ok_or(CircleError::NotInitialized)?;
        if (circle.payout_bitmap & (1u128 << pos)) == 0 {
            if let Some(addr) = pos_to_addr.get(pos) {
                return Ok(addr);
            }
        }
    }
    Err(CircleError::PayoutAlreadyExecuted)
}

pub fn resolve_fixed(env: &Env, circle: &Circle, round: u32) -> Result<Address, CircleError> {
    let pos = round % circle.max_members;
    if (circle.payout_bitmap & (1u128 << pos)) != 0 {
        return Err(CircleError::PayoutAlreadyExecuted);
    }
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut pos_to_addr: Map<u32, Address> = Map::new(env);
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status == MEMBER_ACTIVE {
            pos_to_addr.set(m.position, m.address.clone());
        }
    }
    pos_to_addr.get(pos).ok_or(CircleError::NotMember)
}

pub fn resolve_auction(
    env: &Env,
    circle: &Circle,
    round: u32,
) -> Result<(Address, u32), CircleError> {
    let bids: Vec<AuctionBid> = env
        .storage()
        .persistent()
        .get(&DataKey::Bids)
        .unwrap_or_else(|| Vec::new(env));
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut max_bps: u32 = 0;
    let mut winner: Option<AuctionBid> = None;
    for i in 0..bids.len() {
        let b = bids.get(i).ok_or(CircleError::NotInitialized)?;
        if b.round == round && b.discount_bips >= max_bps {
            max_bps = b.discount_bips;
            winner = Some(b);
        }
    }
    let winner_bid = winner.ok_or(CircleError::VoteQuorumNotMet)?;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == winner_bid.bidder {
            if (circle.payout_bitmap & (1u128 << m.position)) != 0 {
                return Err(CircleError::PayoutAlreadyExecuted);
            }
            return Ok((winner_bid.bidder, winner_bid.discount_bips));
        }
    }
    Err(CircleError::NotMember)
}

/// Computes a member's quadratic voting weight for a round.
///
/// The weight is `max(1, isqrt(power))` where `power` is the member's
/// time-weighted stake in the round's pool (mirroring the distribution formula
/// used in `trigger_payout`, i.e. `amount * time_held` summed over the member's
/// contributions that round). Because influence grows with the square root of
/// the stake, buying influence is quadratically expensive — a voter who wants
/// double the weight must quadruple their contribution. Every active member
/// retains a baseline weight of 1, so votes never require a contribution.
fn vote_weight_of(env: &Env, round: u32, member: &Address) -> Result<u128, CircleError> {
    let all: Vec<Contribution> = env
        .storage()
        .persistent()
        .get(&DataKey::Contributions)
        .unwrap_or_else(|| Vec::new(env));
    let now = env.ledger().timestamp();
    let mut power: u128 = 0;
    for i in 0..all.len() {
        let c = all.get(i).ok_or(CircleError::VecAccessError)?;
        if c.round == round && c.member == *member {
            let time_held = (now as u128).saturating_sub(c.timestamp as u128);
            power = power.saturating_add((c.amount as u128).saturating_mul(time_held));
        }
    }
    let weight = math::isqrt(power as i128).max(1);
    Ok(weight as u128)
}

pub fn resolve_vote(env: &Env, circle: &Circle, round: u32) -> Result<Address, CircleError> {
    let votes: Vec<VoteEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::Votes)
        .unwrap_or_else(|| Vec::new(env));
    let members: Vec<Member> = env
        .storage()
        .persistent()
        .get(&DataKey::Members)
        .ok_or(CircleError::NotInitialized)?;
    let mut eligible_weight: u128 = 0;
    let mut weight_of: Map<Address, u128> = Map::new(env);
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status == MEMBER_ACTIVE {
            let w = vote_weight_of(env, round, &m.address)?;
            eligible_weight = eligible_weight.saturating_add(w);
            weight_of.set(m.address.clone(), w);
        }
    }
    let quorum = eligible_weight
        .checked_div(2)
        .unwrap_or(0)
        .saturating_add(1);
    let mut tally: Map<Address, u128> = Map::new(env);
    let mut cast_weight: u128 = 0;
    for i in 0..votes.len() {
        let v = votes.get(i).ok_or(CircleError::NotInitialized)?;
        if v.round == round {
            let w = weight_of.get(v.voter.clone()).unwrap_or(0);
            if w == 0 {
                continue;
            }
            cast_weight = cast_weight.saturating_add(w);
            let c = tally.get(v.vote_for.clone()).unwrap_or(0);
            tally.set(v.vote_for.clone(), c.saturating_add(w));
        }
    }
    if cast_weight < quorum {
        return Err(CircleError::VoteQuorumNotMet);
    }
    let mut best_addr: Option<Address> = None;
    let mut best_weight: u128 = 0;
    for (addr, count) in tally.iter() {
        if count > best_weight {
            best_weight = count;
            best_addr = Some(addr);
        }
    }
    let winner = best_addr.ok_or(CircleError::VoteQuorumNotMet)?;
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address == winner {
            if (circle.payout_bitmap & (1u128 << m.position)) != 0 {
                return Err(CircleError::PayoutAlreadyExecuted);
            }
            return Ok(winner);
        }
    }
    Err(CircleError::NotMember)
}
