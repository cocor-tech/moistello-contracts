//! Issue #332 — time-weighted contribution scoring.
//!
//! `Contribution.time_weight` used to be the raw block timestamp, which carries
//! no information about *when within the payment window* a member paid. It is
//! now the fraction of the round's payment window that was still open when the
//! contribution landed, in basis points:
//!
//! ```text
//! weight = 0                                   if now >= deadline
//! weight = (deadline - now) * 10_000 / window  otherwise
//! deadline = circle.started_at + circle.contribution_deadline_seconds
//! ```
//!
//! The circle passes that weight to the reputation registry, which prorates the
//! contribution's reputation award by it (`10 * weight / 10_000` base points, so
//! a full-weight contribution still earns the historical flat award of 10).
//! Early capital is what funded the round's payout, so it is scored higher than
//! capital that only arrived just before the deadline.

#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env, String};

use crate::contract::compute_time_weight;
use crate::types::{CircleConfig, TIME_WEIGHT_BPS_MAX};
use crate::{Circle, CircleArgs, CircleClient, CircleError};

const CONTRIBUTION: i128 = 100_0000000;
/// Payment window used by these tests.
const WINDOW: u64 = 100_000;
const GRACE: u64 = 10_000;

fn build_config(env: &Env, organizer: &Address, token: &Address, window: u64, grace: u64) -> CircleConfig {
    CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(env, "Time Weight Circle"),
        contribution_amount: CONTRIBUTION,
        max_members: 2u32,
        payout_type: 1u32,
        total_rounds: 3u32,
        contribution_deadline_seconds: window,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: grace,
        max_strikes: 3u32,
        slug: String::from_str(env, "time-weight"),
    }
}

/// Deploys a circle whose members join at ledger time 0 (so `started_at` is 0
/// and the round deadline is exactly `window`).
fn deploy_active<'a>(
    env: &'a Env,
    window: u64,
    grace: u64,
) -> (CircleClient<'a>, Address, Address, Address) {
    let organizer = Address::generate(env);
    let token_issuer = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(token_issuer).address();
    let config = build_config(env, &organizer, &token, window, grace);
    let id = env.register(Circle, CircleArgs::__constructor(&organizer, &organizer, &config));
    let client = CircleClient::new(env, &id);

    env.ledger().with_mut(|l| {
        l.timestamp = 0;
    });
    let m1 = Address::generate(env);
    let m2 = Address::generate(env);
    for member in [&m1, &m2] {
        StellarAssetClient::new(env, &token).mint(member, &CONTRIBUTION);
        client.join(member);
    }
    (client, token, m1, m2)
}

/// Records a contribution at `at` and returns the stored time weight.
fn weight_at(env: &Env, window: u64, grace: u64, at: u64) -> u64 {
    let (client, _token, m1, _m2) = deploy_active(env, window, grace);
    env.ledger().with_mut(|l| {
        l.timestamp = at;
    });
    client.contribute(&m1, &CONTRIBUTION, &0u32);
    let recorded = client.get_contributions(&m1, &0u32, &1u32);
    recorded.get(0).expect("contribution recorded").time_weight
}

// ── formula unit tests ────────────────────────────────────────────────────────

fn sample_circle(env: &Env, started_at: u64, window: u64) -> crate::types::Circle {
    let token = Address::generate(env);
    let organizer = Address::generate(env);
    crate::types::Circle {
        id: Address::generate(env),
        token,
        name: String::from_str(env, "sample"),
        organizer,
        factory: Address::generate(env),
        contribution_amount: CONTRIBUTION,
        max_members: 2,
        member_count: 2,
        payout_type: 1,
        total_rounds: 3,
        current_round: 0,
        status: 1,
        started_at,
        created_at: started_at,
        contribution_deadline_seconds: window,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 0,
        grace_period_seconds: 0,
        max_strikes: 3,
        payout_bitmap: 0,
        total_payouts: 0,
        total_fees: 0,
        slug: String::from_str(env, "sample"),
        health_score: 0,
    }
}

#[test]
fn formula_is_full_weight_at_window_open_and_zero_at_deadline() {
    let env = Env::default();
    let circle = sample_circle(&env, 0, WINDOW);
    assert_eq!(compute_time_weight(&circle, 0), TIME_WEIGHT_BPS_MAX);
    assert_eq!(compute_time_weight(&circle, WINDOW), 0);
    assert_eq!(compute_time_weight(&circle, WINDOW + 1), 0);
    assert_eq!(compute_time_weight(&circle, u64::MAX), 0);
}

#[test]
fn formula_decays_linearly_across_the_window() {
    let env = Env::default();
    let circle = sample_circle(&env, 0, WINDOW);
    assert_eq!(compute_time_weight(&circle, 25_000), 7_500);
    assert_eq!(compute_time_weight(&circle, 50_000), 5_000);
    assert_eq!(compute_time_weight(&circle, 75_000), 2_500);
    assert_eq!(compute_time_weight(&circle, 90_000), 1_000);
    // Flooring: the last instant of the window is worth (almost) nothing.
    assert_eq!(compute_time_weight(&circle, 99_999), 0);
}

#[test]
fn formula_handles_zero_window_and_overflowing_deadline() {
    let env = Env::default();
    // No window configured: every accepted contribution is full weight.
    let open = sample_circle(&env, 0, 0);
    assert_eq!(compute_time_weight(&open, 0), TIME_WEIGHT_BPS_MAX);
    assert_eq!(compute_time_weight(&open, u64::MAX), TIME_WEIGHT_BPS_MAX);

    // started_at + window overflows u64: weight must be 0, never a panic.
    let overflowing = sample_circle(&env, u64::MAX, WINDOW);
    assert_eq!(compute_time_weight(&overflowing, u64::MAX), 0);
}

#[test]
fn formula_respects_a_shifted_round_start() {
    let env = Env::default();
    let circle = sample_circle(&env, 1_000, WINDOW);
    // Halfway through a window that opened at 1_000.
    assert_eq!(compute_time_weight(&circle, 51_000), 5_000);
    // Still full weight at the moment the window opens.
    assert_eq!(compute_time_weight(&circle, 1_000), TIME_WEIGHT_BPS_MAX);
}

// ── integration through `contribute` ─────────────────────────────────────────

#[test]
fn early_contribution_records_full_weight() {
    let env = Env::default();
    env.mock_all_auths();
    assert_eq!(weight_at(&env, WINDOW, GRACE, 0), 10_000);
}

#[test]
fn later_submissions_record_proportionally_less_weight() {
    let env = Env::default();
    env.mock_all_auths();
    assert_eq!(weight_at(&env, WINDOW, GRACE, 25_000), 7_500);
    assert_eq!(weight_at(&env, WINDOW, GRACE, 50_000), 5_000);
    assert_eq!(weight_at(&env, WINDOW, GRACE, 90_000), 1_000);
}

#[test]
fn graceful_late_contribution_is_accepted_with_zero_weight() {
    let env = Env::default();
    env.mock_all_auths();
    assert_eq!(weight_at(&env, WINDOW, GRACE, WINDOW + 1), 0);
    assert_eq!(weight_at(&env, WINDOW, GRACE, WINDOW + GRACE), 0);
}

#[test]
fn contribution_after_the_grace_period_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _token, m1, _m2) = deploy_active(&env, WINDOW, GRACE);
    env.ledger().with_mut(|l| {
        l.timestamp = WINDOW + GRACE + 1;
    });
    assert_eq!(
        client.try_contribute(&m1, &CONTRIBUTION, &0u32),
        Err(Ok(CircleError::PaymentDeadlinePassed))
    );
}

#[test]
fn windowless_circles_keep_full_weight() {
    let env = Env::default();
    env.mock_all_auths();
    assert_eq!(weight_at(&env, 0, 0, 0), 10_000);
    assert_eq!(weight_at(&env, 0, 0, 9_999_999), 10_000);
}

#[test]
fn on_time_flag_and_weight_agree_on_what_counts_as_late() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _token, m1, _m2) = deploy_active(&env, WINDOW, GRACE);

    env.ledger().with_mut(|l| {
        l.timestamp = 50_000;
    });
    client.contribute(&m1, &CONTRIBUTION, &0u32);
    let on_time = client.get_contributions(&m1, &0u32, &1u32).get(0).unwrap();
    assert!(on_time.on_time);
    assert_eq!(on_time.time_weight, 5_000);
}

// ── reputation-registry integration ──────────────────────────────────────────

/// Reads the reputation score that the circle contract maintains for `member`
/// (the circle calls the registry's scoring module against its own storage, the
/// same way it did before this change).
fn circle_side_score(env: &Env, circle: &Address, member: &Address) -> u32 {
    env.as_contract(circle, || reputation_registry::storage::get_score(env, member))
}

#[test]
fn early_contributions_earn_more_reputation_than_late_ones() {
    // Same circle shape and same member behaviour — only the submission time
    // differs, and that difference must show up in the earned reputation.
    let early_env = Env::default();
    early_env.mock_all_auths();
    let (early_client, _t1, early_member, _) = deploy_active(&early_env, WINDOW, GRACE);
    early_env.ledger().with_mut(|l| {
        l.timestamp = 0;
    });
    early_client.contribute(&early_member, &CONTRIBUTION, &0u32);
    let early_score = circle_side_score(&early_env, &early_client.address, &early_member);

    let late_env = Env::default();
    late_env.mock_all_auths();
    let (late_client, _t2, late_member, _) = deploy_active(&late_env, WINDOW, GRACE);
    late_env.ledger().with_mut(|l| {
        l.timestamp = 90_000;
    });
    late_client.contribute(&late_member, &CONTRIBUTION, &0u32);
    let late_score = circle_side_score(&late_env, &late_client.address, &late_member);

    // The time-weighted component is the full historical flat award of 10 base
    // points at window open, and 1 point at 1_000 bps.
    let early_points = early_env.as_contract(&early_client.address, || {
        reputation_registry::storage::get_time_weighted_points(&early_env, &early_member)
    });
    let late_points = late_env.as_contract(&late_client.address, || {
        reputation_registry::storage::get_time_weighted_points(&late_env, &late_member)
    });
    assert_eq!(early_points, 10u64);
    assert_eq!(late_points, 1u64);
    // The streak and volume bonuses are identical for both, so the earlier
    // contribution carries the strictly higher total score.
    assert!(early_score > late_score);
}

#[test]
fn contributions_within_the_grace_period_earn_no_reputation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _token, m1, _m2) = deploy_active(&env, WINDOW, GRACE);
    env.ledger().with_mut(|l| {
        l.timestamp = WINDOW + 1;
    });
    client.contribute(&m1, &CONTRIBUTION, &0u32);
    assert_eq!(circle_side_score(&env, &client.address, &m1), 0u32);
}
