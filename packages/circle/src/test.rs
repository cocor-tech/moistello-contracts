#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, String, Vec};

use crate::{Circle, CircleArgs, CircleClient, CircleError};

fn mint_tokens(env: &Env, token: &Address, recipient: &Address, amount: i128) {
    let token_client = soroban_sdk::token::StellarAssetClient::new(env, token);
    token_client.mint(recipient, &amount);
}

fn create_config(env: &Env, token: &Address) -> crate::types::CircleConfig {
    crate::types::CircleConfig {
        organizer: Address::generate(env),
        token: token.clone(),
        name: String::from_str(env, "Test Circle"),
        contribution_amount: 100_i128,
        max_members: 2,
        payout_type: crate::types::PAYOUT_FIXED,
        total_rounds: 2,
        contribution_deadline_seconds: 60,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 0,
        max_strikes: 3,
        slug: String::from_str(env, "test-circle"),
        payout_cooldown_seconds: 0,
    }
}

fn setup_circle(env: &Env) -> (CircleClient<'_>, Address, Address) {
    env.mock_all_auths();
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(token_admin);
    let config = create_config(env, &token);
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    (CircleClient::new(env, &contract_id), admin, token)
}

#[test]
fn test_contribute_rejects_amount_above_circle_max() {
    let env = Env::default();
    let (client, _admin, token) = setup_circle(&env);
    let member = Address::generate(&env);
    let other = Address::generate(&env);

    client.join(&member);
    client.join(&other);

    mint_tokens(&env, &token, &member, 200);
    let result = client.try_contribute(&member, &101_i128, &0_u32);
    assert_eq!(result, Err(Ok(CircleError::ContributionMismatch)));
}

#[test]
fn test_batch_payout_rejects_more_than_ten_recipients() {
    let env = Env::default();
    let (client, admin, _token) = setup_circle(&env);
    let member = Address::generate(&env);
    let other = Address::generate(&env);

    client.join(&member);
    client.join(&other);

    let mut recipients = Vec::new(&env);
    let mut amounts = Vec::new(&env);
    for _ in 0..11 {
        recipients.push_back(Address::generate(&env));
        amounts.push_back(1_i128);
    }

    let result = client.try_batch_payout(&admin, &recipients, &amounts, &0_u32);
    assert_eq!(result, Err(Ok(CircleError::InvalidAmount)));
}

#[test]
fn test_trigger_payout_enforces_cooldown_between_rounds() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let config = crate::types::CircleConfig {
        payout_cooldown_seconds: 3_600,
        ..create_config(&env, &token)
    };
    let admin = config.organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(&env, &contract_id);

    let member = Address::generate(&env);
    let other = Address::generate(&env);
    client.join(&member);
    client.join(&other);

    mint_tokens(&env, &token, &member, 100);
    mint_tokens(&env, &token, &other, 100);
    client.contribute(&member, &100_i128, &0_u32);
    client.contribute(&other, &100_i128, &0_u32);
    client.trigger_payout(&admin, &0_u32);

    mint_tokens(&env, &token, &member, 100);
    mint_tokens(&env, &token, &other, 100);
    client.contribute(&member, &100_i128, &1_u32);
    client.contribute(&other, &100_i128, &1_u32);

    let result = client.try_trigger_payout(&admin, &1_u32);
    assert_eq!(result, Err(Ok(CircleError::PayoutCooldownActive)));

    env.ledger().set_timestamp(env.ledger().timestamp() + 3_600);
    client.trigger_payout(&admin, &1_u32);
}

#[test]
fn test_batch_payout_happy_path() {
    let env = Env::default();
    let (client, admin, token) = setup_circle(&env);
    let member_one = Address::generate(&env);
    let member_two = Address::generate(&env);

    client.join(&member_one);
    client.join(&member_two);

    mint_tokens(&env, &token, &member_one, 100);
    mint_tokens(&env, &token, &member_two, 100);
    client.contribute(&member_one, &100_i128, &0_u32);
    client.contribute(&member_two, &100_i128, &0_u32);

    let mut recipients = Vec::new(&env);
    recipients.push_back(member_one.clone());
    recipients.push_back(member_two.clone());

    let mut amounts = Vec::new(&env);
    amounts.push_back(80_i128);
    amounts.push_back(120_i128);

    client.batch_payout(&admin, &recipients, &amounts, &0_u32);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&member_one), 80_i128);
    assert_eq!(token_client.balance(&member_two), 120_i128);
}
