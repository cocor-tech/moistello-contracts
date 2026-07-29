#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Events;
use soroban_sdk::{Address, Env, IntoVal, Symbol, TryIntoVal};
use crate::{Treasury, TreasuryClient};
use crate::types::TreasuryError;

fn mint_tokens(env: &Env, token: &Address, recipient: &Address, amount: i128) {
    let token_client = soroban_sdk::token::StellarAssetClient::new(env, token);
    token_client.mint(recipient, &amount);
}

fn setup(env: &Env) -> (TreasuryClient<'static>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(Treasury, ());
    let client = TreasuryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(token_admin);
    client.init(&admin, &token);
    (client, admin, token)
}

#[test]
fn test_init_sets_admin_and_zero_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Treasury, ());
    let client = TreasuryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    client.init(&admin, &token);

    assert_eq!(client.get_balance(), 0);
    assert_eq!(client.get_deposits().len(), 0);
}

#[test]
fn test_deposit_increases_balance() {
    let env = Env::default();
    let (client, _admin, token) = setup(&env);
    let from = Address::generate(&env);
    let circle_id = Address::generate(&env);

    mint_tokens(&env, &token, &from, 1000i128);
    client.deposit_fee(&from, &1000i128, &circle_id);

    assert_eq!(client.get_balance(), 1000);
}

#[test]
fn test_deposit_records_entry() {
    let env = Env::default();
    let (client, _admin, token) = setup(&env);
    let from = Address::generate(&env);
    let circle_id = Address::generate(&env);

    mint_tokens(&env, &token, &from, 500i128);
    client.deposit_fee(&from, &500i128, &circle_id);

    let deposits = client.get_deposits();
    assert_eq!(deposits.len(), 1);
    assert_eq!(deposits.get(0).unwrap().from, from);
    assert_eq!(deposits.get(0).unwrap().amount, 500);
}

#[test]
fn test_deposit_rejects_zero_amount() {
    let env = Env::default();
    let (client, _admin, token) = setup(&env);
    let from = Address::generate(&env);
    let circle_id = Address::generate(&env);

    let result = client.try_deposit_fee(&from, &0i128, &circle_id);
    assert_eq!(result, Err(Ok(TreasuryError::InvalidAmount)));
}

#[test]
fn test_deposit_rejects_negative_amount() {
    let env = Env::default();
    let (client, _admin, token) = setup(&env);
    let from = Address::generate(&env);
    let circle_id = Address::generate(&env);

    let result = client.try_deposit_fee(&from, &-100i128, &circle_id);
    assert_eq!(result, Err(Ok(TreasuryError::InvalidAmount)));
}

#[test]
fn test_withdraw_decreases_balance() {
    let env = Env::default();
    let (client, admin, token) = setup(&env);
    let from = Address::generate(&env);
    let circle_id = Address::generate(&env);

    mint_tokens(&env, &token, &from, 2000i128);
    client.deposit_fee(&from, &2000i128, &circle_id);
    client.withdraw(&admin, &from, &500i128);

    assert_eq!(client.get_balance(), 1500);
}

#[test]
fn test_withdraw_rejects_insufficient_balance() {
    let env = Env::default();
    let (client, admin, token) = setup(&env);
    let to = Address::generate(&env);

    let result = client.try_withdraw(&admin, &to, &100i128);
    assert_eq!(result, Err(Ok(TreasuryError::InsufficientBalance)));
}

#[test]
fn test_withdraw_rejects_zero_amount() {
    let env = Env::default();
    let (client, admin, token) = setup(&env);
    let to = Address::generate(&env);

    let result = client.try_withdraw(&admin, &to, &0i128);
    assert_eq!(result, Err(Ok(TreasuryError::InvalidAmount)));
}

#[test]
fn test_withdraw_unauthorized() {
    let env = Env::default();
    let (client, _admin, token) = setup(&env);
    let stranger = Address::generate(&env);
    let to = Address::generate(&env);

    let result = client.try_withdraw(&stranger, &to, &100i128);
    assert!(result.is_err());
}

/* #[test]
fn test_deposit_emits_event() {
    let env = Env::default();
    let (client, _admin, token) = setup(&env);
    let from = Address::generate(&env);
    let circle_id = Address::generate(&env);

    mint_tokens(&env, &token, &from, 1000i128);
    client.deposit_fee(&from, &1000i128, &circle_id);

    let events = env.events().all();
    let last = events.last().unwrap();
    let (_id, topics, _data) = last;
    let topic0: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic0, Symbol::new(&env, "deposit"));
} */

#[test]
fn test_multiple_deposits_accumulate() {
    let env = Env::default();
    let (client, _admin, token) = setup(&env);
    let from = Address::generate(&env);
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);

    mint_tokens(&env, &token, &from, 1000i128);
    client.deposit_fee(&from, &1000i128, &c1);
    mint_tokens(&env, &token, &from, 2000i128);
    client.deposit_fee(&from, &2000i128, &c2);
    mint_tokens(&env, &token, &from, 500i128);
    client.deposit_fee(&from, &500i128, &c1);

    assert_eq!(client.get_balance(), 3500);
    assert_eq!(client.get_deposits().len(), 3);
}

#[test]
fn test_pause_blocks_deposit() {
    let env = Env::default();
    let (client, admin, token) = setup(&env);
    let from = Address::generate(&env);
    let circle_id = Address::generate(&env);

    client.pause(&admin);
    let result = client.try_deposit_fee(&from, &100i128, &circle_id);
    assert_eq!(result, Err(Ok(TreasuryError::ContractPaused)));

    client.unpause(&admin);
    mint_tokens(&env, &token, &from, 100i128);
    assert!(client.try_deposit_fee(&from, &100i128, &circle_id).is_ok());
}

#[test]
fn test_pause_blocks_withdraw() {
    let env = Env::default();
    let (client, admin, token) = setup(&env);
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let circle_id = Address::generate(&env);

    mint_tokens(&env, &token, &from, 1000i128);
    client.deposit_fee(&from, &1000i128, &circle_id);
    client.pause(&admin);

    let result = client.try_withdraw(&admin, &to, &100i128);
    assert_eq!(result, Err(Ok(TreasuryError::ContractPaused)));
}



