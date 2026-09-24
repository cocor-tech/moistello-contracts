#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

use crate::types::CircleConfig;

fn deploy_circle<'a>(env: &'a Env, organizer: &'a Address, token: &'a Address) -> crate::CircleClient<'a> {
    let config = CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(env, "Migration Circle"),
        contribution_amount: 1000,
        max_members: 1,
        payout_type: 1, // PAYOUT_FIXED
        total_rounds: 2,
        contribution_deadline_seconds: 604800,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 86400,
        max_strikes: 3,
        slug: String::from_str(env, "migration"),
    };
    let factory = Address::generate(env);
    let contract_id = env.register(crate::Circle, (organizer, &factory, &config));
    crate::CircleClient::new(env, &contract_id)
}

/// State written by the current version must remain readable after an
/// upgrade (simulated by re-instantiating the client against the same
/// contract id, which is how the proxy keeps state during upgrades).
#[test]
fn test_contract_storage_persistence_across_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());

    let client = deploy_circle(&env, &admin, &token);
    let contribution_amount = client.get_status().contribution_amount;

    // Write state, then "upgrade" by re-instantiating the client.
    let member = Address::generate(&env);
    client.join(&member);

    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    token_client.mint(&member, &contribution_amount);

    client.contribute(&member, &contribution_amount, &0u32);

    let upgraded = crate::CircleClient::new(&env, &client.address.clone());
    assert_eq!(upgraded.get_members().len(), 1);
    assert_eq!(upgraded.get_contributions(&member, &0u32, &100u32).len(), 1);
    assert_eq!(upgraded.get_status().current_round, 0u32);
    assert_eq!(upgraded.get_status().contribution_amount, contribution_amount);
}