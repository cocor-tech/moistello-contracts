//! Issue #326 — replay protection for contribution recording.
//!
//! A member must never be able to record two contributions for the same round.
//! The previous implementation answered that question by linearly scanning the
//! whole `Contributions` vector, which is unbounded and grows every round. These
//! tests pin the behaviour of the replacement design:
//!
//! * uniqueness is enforced by a `(member, round)` index with O(1) lookup,
//! * the slot is claimed *before* the token transfer (checks-effects-interactions),
//! * the rejection path never reads the contribution history, so its cost does
//!   not grow as the circle ages,
//! * the index is derived state and self-heals if it is ever missing.

#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
use soroban_sdk::{Address, Env, String};

use crate::types::{CircleConfig, DataKey};
use crate::{Circle, CircleArgs, CircleClient, CircleError};

/// 100 tokens, in stroops.
const CONTRIBUTION: i128 = 100_0000000;

fn build_config(
    env: &Env,
    organizer: &Address,
    token: &Address,
    max_members: u32,
    total_rounds: u32,
    deadline_seconds: u64,
) -> CircleConfig {
    CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(env, "Replay Circle"),
        contribution_amount: CONTRIBUTION,
        max_members,
        payout_type: 1u32, // PAYOUT_FIXED — deterministic recipient order
        total_rounds,
        contribution_deadline_seconds: deadline_seconds,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 86_400u64,
        max_strikes: 3u32,
        slug: String::from_str(env, "replay-circle"),
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

fn fund(env: &Env, token: &Address, member: &Address) {
    StellarAssetClient::new(env, token).mint(member, &1_000_000_0000000i128);
}

fn settled(client: &CircleClient) -> Address {
    client.address.clone()
}

#[test]
fn duplicate_contribution_is_rejected_and_recorded_once() {
    let env = Env::default();
    env.mock_all_auths();
    let organizer = Address::generate(&env);
    let token = deploy_token(&env);
    let config = build_config(&env, &organizer, &token, 2, 5, 604_800);
    let client = deploy(&env, &config);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    fund(&env, &token, &m1);
    fund(&env, &token, &m2);
    client.join(&m1);
    client.join(&m2);

    client.contribute(&m1, &CONTRIBUTION, &0u32);

    // Second attempt for the same round is refused...
    let again = client.try_contribute(&m1, &CONTRIBUTION, &0u32);
    assert_eq!(again, Err(Ok(CircleError::AlreadyContributed)));

    // ...and nothing was recorded or transferred twice.
    assert_eq!(client.get_contributions(&m1, &0u32, &10u32).len(), 1);
    let circle_balance = TokenClient::new(&env, &token).balance(&settled(&client));
    assert_eq!(circle_balance, CONTRIBUTION);
}

#[test]
fn replay_protection_is_per_member_and_per_round() {
    let env = Env::default();
    env.mock_all_auths();
    let organizer = Address::generate(&env);
    let token = deploy_token(&env);
    let config = build_config(&env, &organizer, &token, 2, 2, 604_800);
    let client = deploy(&env, &config);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    fund(&env, &token, &m1);
    fund(&env, &token, &m2);
    client.join(&m1);
    client.join(&m2);

    client.contribute(&m1, &CONTRIBUTION, &0u32);
    // m1 is blocked twice, m2 is still free to pay for the same round.
    assert_eq!(
        client.try_contribute(&m1, &CONTRIBUTION, &0u32),
        Err(Ok(CircleError::AlreadyContributed))
    );
    assert_eq!(
        client.try_contribute(&m1, &CONTRIBUTION, &0u32),
        Err(Ok(CircleError::AlreadyContributed))
    );
    client.contribute(&m2, &CONTRIBUTION, &0u32);

    // Both members recorded exactly one contribution each.
    assert_eq!(client.get_contributions(&m1, &0u32, &10u32).len(), 1);
    assert_eq!(client.get_contributions(&m2, &0u32, &10u32).len(), 1);

    // The settled pool is exactly two contributions.
    let circle_balance = TokenClient::new(&env, &token).balance(&settled(&client));
    assert_eq!(circle_balance, CONTRIBUTION * 2);
}

#[test]
fn contribution_index_advances_with_rounds() {
    let env = Env::default();
    env.mock_all_auths();
    let organizer = Address::generate(&env);
    let token = deploy_token(&env);
    // No deadline window: rounds can advance without touching ledger time.
    let config = build_config(&env, &organizer, &token, 2, 3, 0);
    let client = deploy(&env, &config);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    fund(&env, &token, &m1);
    fund(&env, &token, &m2);
    client.join(&m1);
    client.join(&m2);

    client.contribute(&m1, &CONTRIBUTION, &0u32);
    assert_eq!(
        client.try_contribute(&m1, &CONTRIBUTION, &0u32),
        Err(Ok(CircleError::AlreadyContributed))
    );

    client.contribute(&m2, &CONTRIBUTION, &0u32);
    client.trigger_payout(&organizer, &0u32);
    assert_eq!(client.get_status().current_round, 1u32);

    // A new round is a new slot for the same member...
    client.contribute(&m1, &CONTRIBUTION, &1u32);
    assert_eq!(client.get_contributions(&m1, &0u32, &10u32).len(), 2);
    // ...but it is still a single slot per round.
    assert_eq!(
        client.try_contribute(&m1, &CONTRIBUTION, &1u32),
        Err(Ok(CircleError::AlreadyContributed))
    );
    // The closed round cannot be replayed either.
    assert_eq!(
        client.try_contribute(&m1, &CONTRIBUTION, &0u32),
        Err(Ok(CircleError::RoundNotCurrent))
    );
}

#[test]
fn index_is_rebuilt_when_missing_and_still_blocks_replays() {
    // Migration path: a circle deployed before the index existed (or one whose
    // index entry was lost) rebuilds it from the contribution history once, and
    // the replay guard keeps working afterwards.
    let env = Env::default();
    env.mock_all_auths();
    let organizer = Address::generate(&env);
    let token = deploy_token(&env);
    let config = build_config(&env, &organizer, &token, 2, 5, 604_800);
    let client = deploy(&env, &config);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    fund(&env, &token, &m1);
    fund(&env, &token, &m2);
    client.join(&m1);
    client.join(&m2);
    client.contribute(&m1, &CONTRIBUTION, &0u32);

    // Simulate the pre-index state.
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .remove(&DataKey::ContributionIndex);
    });

    assert_eq!(
        client.try_contribute(&m1, &CONTRIBUTION, &0u32),
        Err(Ok(CircleError::AlreadyContributed))
    );
    assert_eq!(client.get_contributions(&m1, &0u32, &10u32).len(), 1);
    // The rebuild did not block a legitimate first contribution.
    client.contribute(&m2, &CONTRIBUTION, &0u32);
    assert_eq!(client.get_contributions(&m2, &0u32, &10u32).len(), 1);
}

#[test]
fn rejected_replay_does_not_scan_contribution_history() {
    // Benchmark for the acceptance criterion "benchmark gas for the new lookup
    // vs current linear scan". The rejected path answers the uniqueness question
    // without reading the (unbounded) contribution history, so its instruction
    // cost is flat in the number of rounds already recorded — unlike a linear
    // scan, which would grow with every recorded contribution.
    let env = Env::default();
    env.mock_all_auths();
    let organizer = Address::generate(&env);
    let token = deploy_token(&env);
    // One member per round so the fixed payout order never repeats a position
    // (a repeated recipient is refused with `PayoutAlreadyExecuted`), which lets
    // every round advance and the contribution history grow across the run.
    let config = build_config(&env, &organizer, &token, 5, 5, 0);
    let client = deploy(&env, &config);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    let m4 = Address::generate(&env);
    let m5 = Address::generate(&env);
    for member in [&m1, &m2, &m3, &m4, &m5] {
        fund(&env, &token, member);
        client.join(member);
    }

    let mut measured: Vec<u64> = Vec::new();
    for round in 0..5u32 {
        client.contribute(&m1, &CONTRIBUTION, &round);
        // Measure the *rejected* duplicate for this round: this is the call a
        // front-runner would have to pay for.
        assert_eq!(
            client.try_contribute(&m1, &CONTRIBUTION, &round),
            Err(Ok(CircleError::AlreadyContributed))
        );
        let instructions = env.cost_estimate().resources().instructions as u64;
        measured.push(instructions);

        for member in [&m2, &m3, &m4, &m5] {
            client.contribute(member, &CONTRIBUTION, &round);
        }
        client.trigger_payout(&organizer, &round);
    }

    let first = measured[0];
    let last = measured[4];
    // The contribution history grows fivefold between the first and the last
    // measurement; a linear scan would grow with it. Allow generous headroom for
    // the (bounded) index map read while still failing on linear growth.
    assert!(
        last <= first + first / 2,
        "replay rejection cost grew with history: first={first}, last={last}"
    );
    // Issue #326 gas budget for `contribute` is 80_000 stroops, i.e. well under
    // 32M modelled instructions at the network's fee rate.
    assert!(
        last < 32_000_000,
        "replay rejection over budget: {last} instructions"
    );
}
