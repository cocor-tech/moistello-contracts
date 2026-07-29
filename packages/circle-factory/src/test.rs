#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, BytesN, Env, IntoVal, Symbol, TryIntoVal};
use crate::{CircleFactory, CircleFactoryClient, CircleConfig, CircleRegistry, FactoryError};

fn install_wasm_hash(env: &Env) -> BytesN<32> {
    let hash = env.register_contract_wasm(CircleFactory);
    hash
}

fn sample_config(env: &Env, organizer: &Address) -> CircleConfig {
    CircleConfig {
        organizer: organizer.clone(),
        token: Address::generate(env),
        name: soroban_sdk::String::from_str(env, "Test Circle"),
        contribution_amount: 100i128,
        max_members: 10u32,
        payout_type: 0u32,
        total_rounds: 5u32,
        contribution_deadline_seconds: 86400u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 3600u64,
        max_strikes: 3u32,
        slug: soroban_sdk::String::from_str(env, "test-circle"),
    }
}

fn setup(env: &Env) -> (CircleFactoryClient, Address, BytesN<32>) {
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let wh = install_wasm_hash(env);
    client.init(&admin, &500i128, &wh);
    (client, admin, wh)
}

#[test]
fn test_init_stores_admin_and_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    client.init(&admin, &300i128, &wh);

    assert_eq!(client.get_circle_count(), 0);
    let fc = client.get_fee_config();
    assert_eq!(fc.fee_bps, 300);
}

#[test]
fn test_init_rejects_invalid_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let wh = install_wasm_hash(&env);

    let result = client.try_init(&admin, &10001i128, &wh);
    assert_eq!(result, Err(Ok(FactoryError::InvalidFeeBps)));
}

#[test]
fn test_deploy_circle_success() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let organizer = Address::generate(&env);
    let config = sample_config(&env, &organizer);

    let circle_id = client.deploy_circle(&config);

    assert_eq!(client.get_circle_count(), 1);
    let registry = client.get_circles();
    assert_eq!(registry.circles.len(), 1);
    assert_eq!(registry.circles.get(0).unwrap().organizer, organizer);
    assert_eq!(registry.circles.get(0).unwrap().circle_id, circle_id);
}

#[test]
fn test_deploy_circle_rejects_invalid_config() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let mut config = sample_config(&env, &Address::generate(&env));
    config.max_members = 1;

    let result = client.try_deploy_circle(&config);
    assert_eq!(result, Err(Ok(FactoryError::InvalidConfig)));
}

#[test]
fn test_multiple_circles_increment_count() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let org1 = Address::generate(&env);
    let org2 = Address::generate(&env);

    client.deploy_circle(&sample_config(&env, &org1));
    client.deploy_circle(&sample_config(&env, &org2));

    assert_eq!(client.get_circle_count(), 2);
}

#[test]
fn test_deploy_circle_emits_event() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    let organizer = Address::generate(&env);

    client.deploy_circle(&sample_config(&env, &organizer));

    let events = env.events().all();
    let last = events.last().unwrap();
    let (_id, topics, _data) = last;
    let topic0: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic0, Symbol::new(&env, "deploy"));
}

#[test]
fn test_empty_circles() {
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);
    assert_eq!(client.get_circle_count(), 0);
    assert_eq!(client.get_circles().circles.len(), 0);
}

#[test]
fn test_set_fee_config_updates() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);

    client.set_fee_config(&admin, &750i128);

    let fc = client.get_fee_config();
    assert_eq!(fc.fee_bps, 750);
}

#[test]
fn test_set_fee_config_rejects_out_of_bounds() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);

    let r1 = client.try_set_fee_config(&admin, &-1i128);
    assert_eq!(r1, Err(Ok(FactoryError::InvalidFeeBps)));

    let r2 = client.try_set_fee_config(&admin, &10001i128);
    assert_eq!(r2, Err(Ok(FactoryError::InvalidFeeBps)));
}

#[test]
fn test_pause_unpause_blocks_deploy() {
    let env = Env::default();
    let (client, admin, _wh) = setup(&env);
    let config = sample_config(&env, &Address::generate(&env));

    client.pause(&admin);
    let r = client.try_deploy_circle(&config);
    assert_eq!(r, Err(Ok(FactoryError::ContractPaused)));

    client.unpause(&admin);
    assert!(client.try_deploy_circle(&config).is_ok());
}
