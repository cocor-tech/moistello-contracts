//! Issue #330 — quadratic voting for `vote_payout`.
//!
//! Plain majority voting lets a member who can afford it buy the outcome: every
//! head is worth one unit, so N bribes buy N units of influence. The replacement
//! weighs each vote with `1 + min(isqrt(balance / 10^7), 1000)`, so influence
//! grows with the square root of voting power and the price of influence grows
//! quadratically with the amount bought.
//!
//! These tests cover the weighting function, the weighted tally, the weighted
//! quorum, and the invariants that keep the mechanism honest (one vote per
//! member per round, deterministic winner, no disenfranchised member).

#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
use soroban_sdk::{Address, Env, String};

use crate::types::CircleConfig;
use crate::{Circle, CircleArgs, CircleClient, CircleError};

/// 100 tokens, in stroops.
const CONTRIBUTION: i128 = 100_0000000;
/// 10^18 stroops: enough for the quadratic term to hit `MAX_VOTE_WEIGHT`.
const WHALE_BALANCE: i128 = 1_000_000_000_000_000_000;

fn build_config(env: &Env, organizer: &Address, token: &Address, max_members: u32) -> CircleConfig {
    CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(env, "Quadratic Vote Circle"),
        contribution_amount: CONTRIBUTION,
        max_members,
        payout_type: 3u32, // PAYOUT_VOTE
        total_rounds: 3u32,
        contribution_deadline_seconds: 0u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 0u64,
        max_strikes: 3u32,
        slug: String::from_str(env, "quadratic-vote"),
    }
}

fn deploy<'a>(env: &'a Env, config: &CircleConfig) -> CircleClient<'a> {
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, config));
    CircleClient::new(env, &id)
}

fn deploy_token(env: &Env) -> Address {
    let issuer = Address::generate(env);
    env.register_stellar_asset_contract_v2(issuer).address()
}

fn mint(env: &Env, token: &Address, member: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(member, &amount);
}

/// Deploys a funded, active vote circle with `max_members` members.
/// Returns the client plus the member addresses.
fn active_vote_circle<'a>(
    env: &'a Env,
    max_members: u32,
) -> (CircleClient<'a>, Address, soroban_sdk::Vec<Address>) {
    let organizer = Address::generate(env);
    let token = deploy_token(env);
    let config = build_config(env, &organizer, &token, max_members);
    let client = deploy(env, &config);
    let mut members = soroban_sdk::Vec::new(env);
    for _ in 0..max_members {
        let member = Address::generate(env);
        mint(env, &token, &member, CONTRIBUTION);
        client.join(&member);
        members.push_back(member);
    }
    (client, token, members)
}

#[test]
fn member_without_balance_keeps_the_base_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let organizer = Address::generate(&env);
    let token = deploy_token(&env);
    let config = build_config(&env, &organizer, &token, 2);
    let client = deploy(&env, &config);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    client.join(&m1);
    client.join(&m2);

    // m1 holds no tokens at all, so the quadratic term is zero: the weight must
    // still be 1 rather than 0, or a member who has already paid in would be
    // disenfranchised.
    assert_eq!(TokenClient::new(&env, &token).balance(&m1), 0);
    assert_eq!(client.get_vote_weight(&m1), 1u32);
}

#[test]
fn vote_weight_is_quadratic_in_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token, members) = active_vote_circle(&env, 2);
    let m1 = members.get(0).unwrap();

    // 10_000 tokens -> power 10_000 -> isqrt = 100
    mint(&env, &token, &m1, 10_000_0000000i128);
    let small = client.get_vote_weight(&m1);
    assert_eq!(small, 101u32);

    // 100x the balance (10^6 tokens -> isqrt = 1_000) buys ~10x the weight,
    // which is the property that makes vote buying unprofitable.
    mint(&env, &token, &m1, 990_000_0000000i128);
    let large = client.get_vote_weight(&m1);
    assert_eq!(large, 1001u32);
    assert!(
        large <= small * 11,
        "100x balance produced more than ~10x weight: {small} -> {large}"
    );
}

#[test]
fn vote_weight_is_capped() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token, members) = active_vote_circle(&env, 2);
    let m1 = members.get(0).unwrap();

    mint(&env, &token, &m1, WHALE_BALANCE);
    assert_eq!(client.get_vote_weight(&m1), 1001u32);

    // Twice the (already enormous) balance cannot buy more weight than the cap.
    mint(&env, &token, &m1, WHALE_BALANCE);
    assert_eq!(client.get_vote_weight(&m1), 1001u32);
}

#[test]
fn a_member_cannot_vote_twice_even_with_weight() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token, members) = active_vote_circle(&env, 2);
    let m1 = members.get(0).unwrap();
    let m2 = members.get(1).unwrap();

    mint(&env, &token, &m1, WHALE_BALANCE);
    client.vote_payout(&m1, &m1, &0u32);
    assert_eq!(
        client.try_vote_payout(&m1, &m2, &0u32),
        Err(Ok(CircleError::AlreadyVoted))
    );
}

#[test]
fn weighted_quorum_needs_a_majority_of_voting_weight() {
    let env = Env::default();
    env.mock_all_auths();
    let organizer = Address::generate(&env);
    let token = deploy_token(&env);
    let config = build_config(&env, &organizer, &token, 2);
    let client = deploy(&env, &config);
    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..2 {
        let member = Address::generate(&env);
        mint(&env, &token, &member, CONTRIBUTION);
        client.join(&member);
        members.push_back(member);
    }
    let m1 = members.get(0).unwrap();
    let m2 = members.get(1).unwrap();

    // Both members fund the round first, so the circle actually holds the pool
    // a resolved payout has to transfer. Paying in also drains their token
    // balances down to the base vote.
    client.contribute(&m1, &CONTRIBUTION, &0u32);
    client.contribute(&m2, &CONTRIBUTION, &0u32);

    // One unit-weight vote is below the quorum of floor(2/2)+1 = 2.
    client.vote_payout(&m1, &m2, &0u32);
    assert_eq!(
        client.try_trigger_payout(&organizer, &0u32),
        Err(Ok(CircleError::VoteQuorumNotMet))
    );

    // The second vote reaches the weighted quorum and the round resolves.
    client.vote_payout(&m2, &m2, &0u32);
    client.trigger_payout(&organizer, &0u32);
    assert_eq!(client.get_status().current_round, 1u32);
}

#[test]
fn a_heavy_voter_can_reach_the_weighted_quorum_alone() {
    let env = Env::default();
    env.mock_all_auths();
    let organizer = Address::generate(&env);
    let token = deploy_token(&env);
    let config = build_config(&env, &organizer, &token, 3);
    let client = deploy(&env, &config);
    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..3 {
        let member = Address::generate(&env);
        mint(&env, &token, &member, CONTRIBUTION);
        client.join(&member);
        members.push_back(member);
    }
    let whale = members.get(0).unwrap();

    mint(&env, &token, &whale, WHALE_BALANCE);
    // Every member pays in, funding the pool the payout is drawn from.
    for i in 0..members.len() {
        let member = members.get(i).unwrap();
        client.contribute(&member, &CONTRIBUTION, &0u32);
    }
    assert!(client.get_vote_weight(&whale) >= 2u32);

    // Quorum is floor(3/2)+1 = 2 units of weight; one heavy vote clears it.
    client.vote_payout(&whale, &whale, &0u32);
    client.trigger_payout(&organizer, &0u32);
    assert_eq!(client.get_status().current_round, 1u32);
}

#[test]
fn a_coalition_of_small_holders_outweighs_one_whale() {
    // The vote-buying scenario: one member owns most of the token supply and
    // votes for itself. Because the whale's weight is capped at MAX_VOTE_WEIGHT
    // while the other members keep genuine quadratic weight of their own, the
    // coalition wins on weight even though the whale votes first.
    let env = Env::default();
    env.mock_all_auths();
    let organizer = Address::generate(&env);
    let token = deploy_token(&env);
    let config = build_config(&env, &organizer, &token, 3);
    let client = deploy(&env, &config);

    let whale = Address::generate(&env);
    let ally = Address::generate(&env);
    let small = Address::generate(&env);
    for member in [&whale, &ally, &small] {
        mint(&env, &token, member, CONTRIBUTION);
        client.join(member);
    }

    // Re-mint the balances voting power is derived from.
    mint(&env, &token, &whale, WHALE_BALANCE);
    mint(&env, &token, &ally, 100_000_000_000_000i128); // 10^7 tokens

    // Everyone funds the round, so the resolved payout can actually be moved.
    for member in [&whale, &ally, &small] {
        client.contribute(member, &CONTRIBUTION, &0u32);
    }
    // `small` has now paid everything in and holds nothing, so the quadratic
    // term vanishes and only the base vote remains.
    assert_eq!(client.get_vote_weight(&small), 1u32);

    let whale_weight = client.get_vote_weight(&whale);
    let ally_weight = client.get_vote_weight(&ally);
    assert!(ally_weight + 1 > whale_weight);

    client.vote_payout(&whale, &whale, &0u32);
    client.vote_payout(&ally, &ally, &0u32);
    client.vote_payout(&small, &ally, &0u32);

    let ally_before = TokenClient::new(&env, &token).balance(&ally);
    client.trigger_payout(&organizer, &0u32);
    let ally_after = TokenClient::new(&env, &token).balance(&ally);

    // The coalition's nominee received the round's pool, not the whale.
    assert_eq!(ally_after - ally_before, CONTRIBUTION * 3);
    let whale_after = TokenClient::new(&env, &token).balance(&whale);
    assert_eq!(whale_after, WHALE_BALANCE);
}

#[test]
fn equal_weight_voters_keep_the_historical_plurality_behaviour() {
    // With equal balances (the common case) the weighted tally must behave
    // exactly like the old one: the nominee with the most votes wins.
    let env = Env::default();
    env.mock_all_auths();
    let organizer = Address::generate(&env);
    let token = deploy_token(&env);
    let config = build_config(&env, &organizer, &token, 3);
    let client = deploy(&env, &config);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    for member in [&a, &b, &c] {
        mint(&env, &token, member, CONTRIBUTION);
        client.join(member);
    }
    // The circle only accepts contributions once the last member joins.
    for member in [&a, &b, &c] {
        client.contribute(member, &CONTRIBUTION, &0u32);
    }

    client.vote_payout(&a, &b, &0u32);
    client.vote_payout(&b, &b, &0u32);
    client.vote_payout(&c, &a, &0u32);

    let b_before = TokenClient::new(&env, &token).balance(&b);
    client.trigger_payout(&organizer, &0u32);
    let b_after = TokenClient::new(&env, &token).balance(&b);
    assert_eq!(b_after - b_before, CONTRIBUTION * 3);
}
