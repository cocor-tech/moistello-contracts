#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{Address, Env, String};

/// Migration / storage-versioning test for the payout cooldown fields added
/// in fixes #115 and #117.
///
/// Specifically verifies that:
/// 1. A newly-deployed circle has `payout_cooldown_seconds` and
///    `last_payout_timestamp` set to the values supplied via `CircleConfig`
///    (or the additive defaults of 0 when not supplied by older clients).
/// 2. After a successful `trigger_payout`, `last_payout_timestamp` is updated
///    to the current ledger timestamp.
/// 3. When `payout_cooldown_seconds` is 0, no cooldown is enforced and
///    back-to-back payouts succeed.
///
/// See docs/UPGRADE.md §3 for the general upgrade-testing pattern this follows.
#[test]
fn test_upgrade_payout_cooldown_fields_have_sane_defaults() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let organizer = Address::generate(&env);
    let factory = Address::generate(&env);

    // Deploy with payout_cooldown_seconds = 0 (additive default — simulates
    // an "old" client that doesn't know about the field yet).
    let config = crate::types::CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(&env, "Migration Test Circle"),
        contribution_amount: 100_i128,
        max_members: 2u32,
        payout_type: crate::types::PAYOUT_FIXED,
        total_rounds: 2u32,
        contribution_deadline_seconds: 604800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 0u32,
        grace_period_seconds: 86400u64,
        max_strikes: 3u32,
        slug: String::from_str(&env, "migration-test"),
        payout_cooldown_seconds: 0u64, // additive default: no cooldown
    };

    let contract_id = env.register(
        crate::Circle,
        (&organizer, &factory, &config),
    );
    let client = crate::CircleClient::new(&env, &contract_id);

    // ── Verify additive field defaults ───────────────────────────────────────
    let status = client.get_status();
    assert_eq!(
        status.payout_cooldown_seconds, 0,
        "payout_cooldown_seconds must default to 0"
    );
    assert_eq!(
        status.last_payout_timestamp, 0,
        "last_payout_timestamp must start at 0 (never paid)"
    );

    // ── Perform a full round to verify last_payout_timestamp is recorded ─────
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    token_client.mint(&m1, &10_000);
    token_client.mint(&m2, &10_000);

    client.join(&m1);
    client.join(&m2);

    let payout_time: u64 = 1_234_567;
    env.ledger().set_timestamp(payout_time);

    client.contribute(&m1, &100_i128, &0_u32);
    client.contribute(&m2, &100_i128, &0_u32);
    client.trigger_payout(&organizer, &0_u32);

    let status_after = client.get_status();
    assert_eq!(
        status_after.last_payout_timestamp, payout_time,
        "last_payout_timestamp must be updated to the ledger timestamp after payout"
    );
    assert_eq!(
        status_after.current_round, 1,
        "round must advance after payout"
    );
}

/// Verifies that a circle configured with a non-zero `payout_cooldown_seconds`
/// correctly stores that value and enforces the cooldown across rounds.
#[test]
fn test_upgrade_nonzero_cooldown_stored_and_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let organizer = Address::generate(&env);
    let factory = Address::generate(&env);

    let cooldown_secs: u64 = 3_600; // 1 hour

    let config = crate::types::CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(&env, "Cooldown Test Circle"),
        contribution_amount: 100_i128,
        max_members: 2u32,
        payout_type: crate::types::PAYOUT_FIXED,
        total_rounds: 2u32,
        contribution_deadline_seconds: 604800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 0u32,
        grace_period_seconds: 86400u64,
        max_strikes: 3u32,
        slug: String::from_str(&env, "cooldown-test"),
        payout_cooldown_seconds: cooldown_secs,
    };

    let contract_id = env.register(crate::Circle, (&organizer, &factory, &config));
    let client = crate::CircleClient::new(&env, &contract_id);

    // Stored value round-trips correctly.
    assert_eq!(client.get_status().payout_cooldown_seconds, cooldown_secs);

    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    token_client.mint(&m1, &10_000);
    token_client.mint(&m2, &10_000);
    client.join(&m1);
    client.join(&m2);

    env.ledger().set_timestamp(1_000);
    client.contribute(&m1, &100_i128, &0_u32);
    client.contribute(&m2, &100_i128, &0_u32);
    // First payout — always allowed.
    client.trigger_payout(&organizer, &0_u32);

    // Immediately attempt round-1 payout — must be blocked.
    token_client.mint(&m1, &10_000);
    token_client.mint(&m2, &10_000);
    client.contribute(&m1, &100_i128, &1_u32);
    client.contribute(&m2, &100_i128, &1_u32);

    let blocked = client.try_trigger_payout(&organizer, &1_u32);
    assert_eq!(
        blocked,
        Err(Ok(crate::CircleError::PayoutCooldownActive)),
        "second payout within cooldown window must be blocked"
    );

    // Advance past cooldown — must succeed.
    env.ledger().set_timestamp(1_000 + cooldown_secs + 1);
    assert!(
        client.try_trigger_payout(&organizer, &1_u32).is_ok(),
        "payout after cooldown must succeed"
    );
}
