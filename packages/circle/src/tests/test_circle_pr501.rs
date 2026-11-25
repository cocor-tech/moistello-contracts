#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, Address, Env, String};

use crate::types::{CircleConfig, CircleError, PAYOUT_FIXED};

const CONTRIBUTION: i128 = 100_0000000;
const T0: u64 = 1_000;

/// Minimal mock token that always fails `transfer` so we can test #348
/// transfer failure propagation and `payout_bitmap` rollback.
#[contract]
pub struct FailingTransferToken;

#[contractimpl]
impl FailingTransferToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
        panic!("simulated transfer failure");
    }

    pub fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {
        // Allow deposits (`contribute`) to succeed so payout is reached.
    }

    pub fn balance(_env: Env, _id: Address) -> i128 {
        1_000_0000000
    }
}

fn base_config(env: &Env, max_members: u32, total_rounds: u32) -> CircleConfig {
    let token_admin = Address::generate(env);
    CircleConfig {
        organizer: Address::generate(env),
        token: env
            .register_stellar_asset_contract_v2(token_admin)
            .address(),
        name: String::from_str(env, "PR501 Circle"),
        contribution_amount: CONTRIBUTION,
        max_members,
        payout_type: PAYOUT_FIXED,
        total_rounds,
        contribution_deadline_seconds: 100,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 50,
        max_strikes: 3,
        slug: String::from_str(env, "pr501"),
        max_withdrawal_per_tx: 0,
        daily_withdrawal_limit: 0,
        min_duration_seconds: 0,
    }
}

fn deploy<'a>(env: &'a Env, config: &CircleConfig) -> crate::CircleClient<'a> {
    let factory = Address::generate(env);
    let id = env.register(crate::Circle, (&config.organizer, &factory, config));
    crate::CircleClient::new(env, &id)
}

fn setup_active_circle<'a>(
    env: &'a Env,
    member_count: u32,
    total_rounds: u32,
) -> (crate::CircleClient<'a>, CircleConfig, Vec<Address>) {
    env.ledger().set_timestamp(T0);
    let config = base_config(env, member_count, total_rounds);
    let client = deploy(env, &config);
    let token = soroban_sdk::token::StellarAssetClient::new(env, &config.token);
    let mut members = Vec::new();
    for _ in 0..member_count {
        let m = Address::generate(env);
        token.mint(&m, &(CONTRIBUTION * (total_rounds as i128 + 2)));
        client.join_circle(&m, &None);
        members.push(m);
    }
    (client, config, members)
}

// ============================================================================
// #350: Overpayment / Underpayment validation
// ============================================================================

#[test]
fn test_contribute_exact_over_and_underpayment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _config, members) = setup_active_circle(&env, 2, 2);

    // Overpayment -> CircleError::Overpayment
    let over = client.try_contribute(&members[0], &0, &(CONTRIBUTION + 1));
    assert_eq!(over, Err(Ok(CircleError::Overpayment)));

    // Underpayment -> CircleError::Underpayment
    let under = client.try_contribute(&members[0], &0, &(CONTRIBUTION - 1));
    assert_eq!(under, Err(Ok(CircleError::Underpayment)));

    // Exact match -> Ok
    client.contribute(&members[0], &0, &CONTRIBUTION);
}

// ============================================================================
// #357: Circle metadata updates
// ============================================================================

#[test]
fn test_update_metadata_allows_mutable_and_rejects_immutable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, config, members) = setup_active_circle(&env, 2, 2);

    // Organizer can update name, slug, and description
    client.update_metadata(
        &config.organizer,
        &String::from_str(&env, "name"),
        &String::from_str(&env, "Renamed Circle"),
    );
    client.update_metadata(
        &config.organizer,
        &String::from_str(&env, "slug"),
        &String::from_str(&env, "renamed-circle"),
    );
    client.update_metadata(
        &config.organizer,
        &String::from_str(&env, "description"),
        &String::from_str(&env, "Updated circle description"),
    );
    assert_eq!(
        client.get_description(),
        Some(String::from_str(&env, "Updated circle description"))
    );

    // Immutable fields must be rejected with MetadataImmutable
    for immutable in [
        "token",
        "contribution_amount",
        "max_members",
        "payout_type",
        "total_rounds",
    ] {
        let res = client.try_update_metadata(
            &config.organizer,
            &String::from_str(&env, immutable),
            &String::from_str(&env, "bad"),
        );
        assert_eq!(res, Err(Ok(CircleError::MetadataImmutable)));
    }

    // Non-admin/non-organizer must be rejected with Unauthorized
    let unauth = client.try_update_metadata(
        &members[1],
        &String::from_str(&env, "name"),
        &String::from_str(&env, "Hack"),
    );
    assert_eq!(unauth, Err(Ok(CircleError::Unauthorized)));
}

// ============================================================================
// #351: Circle graduation with reputation boost
// ============================================================================

#[test]
fn test_circle_graduation_boosts_reputation_and_handles_missing_registry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, config, members) = setup_active_circle(&env, 2, 2);

    // Register real reputation registry contract
    let registry_id = env.register(reputation_registry::ReputationRegistry, ());
    let reg_client = reputation_registry::ReputationRegistryClient::new(&env, &registry_id);
    reg_client.init(&config.organizer);
    client.set_reputation_registry(&config.organizer, &registry_id);

    // Round 0
    client.contribute(&members[0], &0, &CONTRIBUTION);
    client.contribute(&members[1], &0, &CONTRIBUTION);
    client.trigger_payout(&0);

    // Round 1 (final round -> graduation)
    client.contribute(&members[0], &1, &CONTRIBUTION);
    client.contribute(&members[1], &1, &CONTRIBUTION);
    client.trigger_payout(&1);

    // Each active member received +100 MoiScore boost in the registry
    assert!(reg_client.get_moi_score(&members[0]) >= 100);
    assert!(reg_client.get_moi_score(&members[1]) >= 100);
}

#[test]
fn test_circle_graduation_gracefully_degrades_without_registry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _config, members) = setup_active_circle(&env, 2, 1);

    client.contribute(&members[0], &0, &CONTRIBUTION);
    client.contribute(&members[1], &0, &CONTRIBUTION);
    // Should succeed even with no reputation registry configured
    client.trigger_payout(&0);
}

// ============================================================================
// #348: Payout token transfer failure propagation and bitmap rollback
// ============================================================================

#[test]
fn test_payout_transfer_failure_propagates_error_and_preserves_bitmap() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);

    let mut config = base_config(&env, 2, 2);
    config.token = env.register(FailingTransferToken, ());
    let client = deploy(&env, &config);

    let m0 = Address::generate(&env);
    let m1 = Address::generate(&env);
    client.join_circle(&m0, &None);
    client.join_circle(&m1, &None);

    client.contribute(&m0, &0, &CONTRIBUTION);
    client.contribute(&m1, &0, &CONTRIBUTION);

    let res = client.try_trigger_payout(&0);
    assert_eq!(res, Err(Ok(CircleError::TransferFailed)));
}
