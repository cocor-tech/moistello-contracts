#![cfg(test)]

//! Quadratic-voting tests (#330).
//!
//! Vote weight for a round is `max(1, isqrt(power))` where `power` is the voter's
//! time-weighted stake (`contribution_amount * time_held`), mirroring the pool
//! distribution formula. Because influence grows with the square root of stake,
//! buying more influence is quadratically expensive: a voter must quadruple their
//! stake to double their weight. Quorum is a weighted majority of eligible weight
//! across all active members. Double-casting is blocked by the dedicated
//! `DataKey::VoteExists(voter, round)` entry with a migration fallback.

use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{Address, Env, String, Vec};

use crate::types::{CircleConfig, DataKey, PayoutRecipient, PAYOUT_VOTE};
use crate::{Circle, CircleArgs, CircleClient, CircleError};

/// Per-round contribution amount. 10_0000000 × time_held keeps the powers used
/// below at exact squares, so the expected quadratic weights are exact.
const AMOUNT: i128 = 10_0000000;
/// Tokens minted to each member.
const FUNDING: i128 = 10_000_000_000;

fn create_config(env: &Env, token: &Address, max_members: u32) -> CircleConfig {
    CircleConfig {
        organizer: Address::generate(env),
        token: token.clone(),
        name: String::from_str(env, "Vote Circle"),
        contribution_amount: AMOUNT,
        max_members,
        payout_type: PAYOUT_VOTE,
        total_rounds: 1,
        contribution_deadline_seconds: 604_800,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 86_400,
        max_strikes: 3,
        slug: String::from_str(env, "vote"),
    }
}

/// Deploys an ACTIVE vote circle with `count` funded members; returns the
/// organizer, which is also the admin.
fn setup_vote_circle<'a>(
    env: &'a Env,
    count: u32,
) -> (CircleClient<'a>, Address, Address, Address, Vec<Address>) {
    env.mock_all_auths();

    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let config = create_config(env, &token, count);
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(env, &contract_id);

    let token_client = soroban_sdk::token::StellarAssetClient::new(env, &token);
    let mut members = Vec::new(env);
    for _ in 0..count {
        let member = Address::generate(env);
        token_client.mint(&member, &FUNDING);
        client.join(&member);
        members.push_back(member);
    }

    (client, contract_id, admin, token, members)
}

/// Reads the `(voter, round)` uniqueness entry straight from contract storage.
fn vote_entry(env: &Env, contract_id: &Address, voter: &Address, round: u32) -> Option<bool> {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::VoteExists(voter.clone(), round))
    })
}

/// The winner recorded in the first payout entry for a round.
fn payout_winner(env: &Env, contract_id: &Address, round: u32) -> Address {
    env.as_contract(contract_id, || {
        let payouts: Vec<PayoutRecipient> = env
            .storage()
            .persistent()
            .get(&DataKey::Payouts)
            .unwrap_or_else(|| Vec::new(env));
        for p in payouts.iter() {
            if p.round == round {
                return p.recipient;
            }
        }
        panic!("no payout recorded for round {round}");
    })
}

// ── Double-vote protection (O(1) entry + migration fallback) ─────────────────

#[test]
fn test_vote_sets_uniqueness_entry_and_rejects_double_cast() {
    let env = Env::default();
    let (client, contract_id, _admin, _token, members) = setup_vote_circle(&env, 2);
    let voter = members.get(0).unwrap();
    let nominee = members.get(1).unwrap();

    assert_eq!(vote_entry(&env, &contract_id, &voter, 0u32), None);

    client.vote_payout(&voter, &nominee, &0u32);
    assert_eq!(vote_entry(&env, &contract_id, &voter, 0u32), Some(true));

    assert_eq!(
        client.try_vote_payout(&voter, &nominee, &0u32),
        Err(Ok(CircleError::AlreadyVoted))
    );
}

#[test]
fn test_vote_double_cast_blocked_without_entry_after_migration() {
    let env = Env::default();
    let (client, contract_id, _admin, _token, members) = setup_vote_circle(&env, 2);
    let voter = members.get(0).unwrap();
    let nominee = members.get(1).unwrap();

    // Simulate a circle deployed before the `VoteExists` entry existed.
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::VoteExists(voter.clone(), 0u32));
    });

    client.vote_payout(&voter, &nominee, &0u32);
    assert_eq!(
        client.try_vote_payout(&voter, &nominee, &0u32),
        Err(Ok(CircleError::AlreadyVoted))
    );
}

// ── Quadratic weighting ───────────────────────────────────────────────────────

/// A single "whale" who contributed earliest (250s of time-held → power 2.5e9,
/// weight isqrt = 50_000) is outvoted by three ordinary members who contributed
/// later (90s each → power 9e8, weight isqrt = 30_000 each → 90_000 total).
/// Linearly the whale's 2.5e9 stake beats the coalition's 9e8; quadratically the
/// whale's influence is capped at sqrt(2.5e9). Doubling the decisive weight
/// would have required quadrupling the stake.
#[test]
fn test_quadratic_weight_caps_whale_influence() {
    let env = Env::default();
    let (client, contract_id, admin, _token, members) = setup_vote_circle(&env, 4);
    let whale = members.get(0).unwrap();
    let small_a = members.get(1).unwrap();
    let small_b = members.get(2).unwrap();
    let small_c = members.get(3).unwrap();

    // Early whale contribution: time_held = 250 at resolve time → power 2.5e9
    // (a perfect square: isqrt = 50_000).
    env.ledger().set_timestamp(1_000);
    client.contribute(&whale, &AMOUNT, &0u32);

    // Late coalition contributions: time_held = 90 → power 9e8 (isqrt = 30_000).
    env.ledger().set_timestamp(1_160);
    client.contribute(&small_a, &AMOUNT, &0u32);
    client.contribute(&small_b, &AMOUNT, &0u32);
    client.contribute(&small_c, &AMOUNT, &0u32);

    client.vote_payout(&whale, &whale, &0u32);
    client.vote_payout(&small_a, &small_b, &0u32);
    client.vote_payout(&small_b, &small_b, &0u32);
    client.vote_payout(&small_c, &small_b, &0u32);

    env.ledger().set_timestamp(1_250);
    client.trigger_payout(&admin, &0u32);

    assert_eq!(payout_winner(&env, &contract_id, 0u32), small_b);
}

/// Quorum is a weighted majority of eligible weight. With four equal-weight
/// voters, two votes (exactly half of eligible weight) do NOT meet the 1/2 + 1
/// threshold; three do.
#[test]
fn test_weighted_quorum_requires_majority_of_eligible_weight() {
    let env = Env::default();
    let (client, contract_id, admin, _token, members) = setup_vote_circle(&env, 4);

    env.ledger().set_timestamp(1_000);
    for i in 0..4 {
        let member = members.get(i).unwrap();
        client.contribute(&member, &AMOUNT, &0u32);
    }

    let target = members.get(0).unwrap();
    let voter_a = members.get(1).unwrap();
    let voter_b = members.get(2).unwrap();

    client.vote_payout(&voter_a, &target, &0u32);
    client.vote_payout(&voter_b, &target, &0u32);

    // Advance time so each voter carries a non-unit (quadratic) weight.
    env.ledger().set_timestamp(1_060);

    // Exactly half of eligible weight cast → below the 1/2 + 1 threshold.
    assert_eq!(client.try_trigger_payout(&admin, &0u32), Err(Ok(CircleError::VoteQuorumNotMet)));

    // A third voter tips cast weight over the threshold.
    let voter_c = members.get(3).unwrap();
    client.vote_payout(&voter_c, &target, &0u32);
    client.trigger_payout(&admin, &0u32);

    assert_eq!(payout_winner(&env, &contract_id, 0u32), target);
}

/// With equal weights, the tally resolves in favour of the majority — each
/// member may cast exactly one (sectioned) vote per round.
#[test]
fn test_equal_weight_outcome_favors_majority() {
    let env = Env::default();
    let (client, contract_id, admin, _token, members) = setup_vote_circle(&env, 5);

    env.ledger().set_timestamp(1_000);
    for i in 0..5 {
        let member = members.get(i).unwrap();
        client.contribute(&member, &AMOUNT, &0u32);
    }

    let target = members.get(0).unwrap();
    let dissent = members.get(4).unwrap();
    // 3 of 5 vote for the same nominee; the fifth dissents.
    for i in 1..4 {
        client.vote_payout(&members.get(i).unwrap(), &target, &0u32);
    }
    client.vote_payout(&dissent, &members.get(1).unwrap(), &0u32);

    env.ledger().set_timestamp(1_060);
    client.trigger_payout(&admin, &0u32);

    assert_eq!(payout_winner(&env, &contract_id, 0u32), target);
}
