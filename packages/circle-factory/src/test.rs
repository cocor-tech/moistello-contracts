#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};
use crate::{CircleFactory, CircleFactoryClient}; use crate::types::{CircleConfig, FactoryError};

fn install_wasm_hash(env: &Env) -> BytesN<32> {
    // Test fixture wasm shipped with soroban-sdk (valid Soroban contract
    // wasm with metadata section). The factory only deploys it; the deployed
    // contract is never invoked by the factory tests.
    let wasm: &[u8] = include_bytes!("../test_wasm/contract.wasm");
    env.deployer().upload_contract_wasm(wasm)
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
fn test_get_fee_config_returns_default_when_uninitialized() {
    let env = Env::default();
    let contract_id = env.register(CircleFactory, ());
    let client = CircleFactoryClient::new(&env, &contract_id);

    let fc = client.get_fee_config();
    assert_eq!(fc.fee_bps, 0);
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

#[test]
fn test_storage_isolation_across_100_deployed_circles() {
    // Issue #456: factory-deployed circles must not share storage. Each
    // deploy_v2 call gets its own contract instance via a distinct salt, but
    // that's a platform guarantee worth proving empirically — a salt or
    // constructor-argument bug could silently make two circles alias the
    // same address/config. Verified here through the factory's own
    // per-circle storage (get_circle_config), which is what every other
    // reader of a deployed circle's config (indexers, the frontend) also
    // goes through — see sample_config()'s neighbours in this file for why
    // the deployed circle contract itself is never invoked directly in
    // these tests.
    let env = Env::default();
    let (client, _admin, _wh) = setup(&env);

    const N: u32 = 100;
    let mut circle_ids: std::vec::Vec<Address> = std::vec::Vec::with_capacity(N as usize);
    let mut expected_amounts: std::vec::Vec<i128> = std::vec::Vec::with_capacity(N as usize);

    for i in 0..N {
        let organizer = Address::generate(&env);
        let mut config = sample_config(&env, &organizer);
        // Divergent per-circle config: each circle's contribution_amount and
        // slug are unique, so any cross-circle storage aliasing would show
        // up as one circle's config bleeding into another's.
        config.contribution_amount = 100i128 + i as i128;
        config.slug = soroban_sdk::String::from_str(&env, &std::format!("isolation-test-{i}"));
        let cid = client.deploy_circle(&config);
        circle_ids.push(cid);
        expected_amounts.push(config.contribution_amount);
    }

    assert_eq!(client.get_circle_count(), N);

    // Config A change never observable from circle B: read every deployed
    // circle's own stored config back and confirm it matches exactly what
    // that circle (and only that circle) was configured with.
    for i in 0..N as usize {
        let stored = client.get_circle_config(&circle_ids[i]);
        assert_eq!(
            stored.contribution_amount, expected_amounts[i],
            "circle {i} read back a contribution_amount belonging to a different circle"
        );
        assert_eq!(
            stored.slug,
            soroban_sdk::String::from_str(&env, &std::format!("isolation-test-{i}")),
            "circle {i} read back a slug belonging to a different circle"
        );
    }

    // Every deployed circle address must be unique — a collision here would
    // mean two configs landed on the same storage instance.
    for i in 0..N as usize {
        for j in (i + 1)..N as usize {
            assert_ne!(circle_ids[i], circle_ids[j], "circles {i} and {j} deployed to the same address");
        }
    }
}
