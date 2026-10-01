//! Quadratic voting for `PAYOUT_VOTE` rounds (issue #330).
//!
//! # Why
//!
//! Plain majority voting gives one unit of influence per head, so the cheapest
//! way to steer a payout is to buy votes. The classic counter-measure is
//! *quadratic voting*: a voter's influence grows with the **square root** of
//! their voting power, so buying 100x the power only buys 10x the influence.
//! The cost of influence therefore grows quadratically while the influence
//! itself grows only linearly in the purchased quantity, which removes the
//! profit from large-scale vote buying.
//!
//! # Formula
//!
//! ```text
//! power_i  = floor(balance_i / VOTE_POWER_UNIT)          // whole token units
//! weight_i = BASE_VOTE_WEIGHT + min(isqrt(power_i), MAX_VOTE_WEIGHT)
//! ```
//!
//! where
//!
//! * `balance_i` is the voter's balance of the circle's settlement token
//!   (`VOTE_POWER_UNIT` = 1 token = 10_000_000 stroops), so power is expressed
//!   in whole tokens rather than stroops.
//! * `BASE_VOTE_WEIGHT` = 1 guarantees that every member keeps exactly one base
//!   vote even with a zero balance. Without it a member who has already paid
//!   their contribution (and therefore holds no tokens) would be disenfranchised.
//! * `MAX_VOTE_WEIGHT` caps the quadratic term so a single address holding an
//!   unbounded balance cannot dominate the tally outright.
//!
//! # Quorum
//!
//! Quorum is measured in *voting weight*, not in raw head count:
//!
//! ```text
//! quorum_weight = floor(active_members / 2) + 1
//! cast_weight   = sum of weight_i over the votes cast this round
//! ```
//!
//! A round resolves only when `cast_weight >= quorum_weight`. Because every
//! member carries at least `BASE_VOTE_WEIGHT`, this is exactly the historical
//! head-count quorum for equal-weight voters and stays unchanged for them,
//! while heavier voters reach the bar with fewer participants — the quorum
//! accounting follows the same weighting as the tally itself.
//!
//! Each member may still vote at most once per round (`CircleError::AlreadyVoted`),
//! so quadratic weighting never lets a single caller stuff the ballot.

use crate::types::{Circle, CircleError};
use common::math;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{Address, Env};

/// Stroops per token unit (Stellar's 7-decimal precision): 1 token = 10^7 stroops.
pub const VOTE_POWER_UNIT: i128 = 10_000_000;

/// Every member always carries this much voting weight, independent of balance.
pub const BASE_VOTE_WEIGHT: u32 = 1;

/// Upper bound on the quadratic term of a single member's vote weight.
pub const MAX_VOTE_WEIGHT: u32 = 1_000;

/// Returns the member's quadratic voting weight for the given circle.
///
/// A token that is missing or cannot be queried (for example an older circle
/// pointing at a non-token address) yields zero power rather than trapping the
/// payout: the voter still keeps their `BASE_VOTE_WEIGHT`.
pub fn vote_weight(env: &Env, circle: &Circle, member: &Address) -> u32 {
    let balance = match TokenClient::new(env, &circle.token).try_balance(member) {
        Ok(Ok(balance)) => balance,
        _ => 0,
    };
    let power: u128 = if balance <= 0 {
        0
    } else {
        (balance / VOTE_POWER_UNIT) as u128
    };
    let quadratic = math::isqrt(power);
    let capped = if quadratic > MAX_VOTE_WEIGHT as u128 {
        MAX_VOTE_WEIGHT as u128
    } else {
        quadratic
    };
    BASE_VOTE_WEIGHT.saturating_add(capped as u32)
}

/// Weighted quorum required for `round` to resolve: `floor(active / 2) + 1`.
pub fn quorum_weight(active_members: u32) -> u128 {
    u128::from(active_members / 2) + 1
}

/// Validates the quadratic-voting invariants for a vote tally.
///
/// Returns `Ok(cast_weight)` when the weighted quorum is met, otherwise
/// `CircleError::VoteQuorumNotMet`.
pub fn check_quorum(cast_weight: u128, active_members: u32) -> Result<u128, CircleError> {
    if cast_weight < quorum_weight(active_members) {
        return Err(CircleError::VoteQuorumNotMet);
    }
    Ok(cast_weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quorum_weight_is_simple_majority() {
        assert_eq!(quorum_weight(0), 1);
        assert_eq!(quorum_weight(1), 1);
        assert_eq!(quorum_weight(2), 2);
        assert_eq!(quorum_weight(3), 2);
        assert_eq!(quorum_weight(4), 3);
        assert_eq!(quorum_weight(100), 51);
    }

    #[test]
    fn check_quorum_rejects_insufficient_weight() {
        assert_eq!(check_quorum(2, 4), Err(CircleError::VoteQuorumNotMet));
        assert_eq!(check_quorum(3, 4), Ok(3));
        // A single heavy voter can satisfy a weighted quorum.
        assert_eq!(check_quorum(51, 100), Ok(51));
    }
}
