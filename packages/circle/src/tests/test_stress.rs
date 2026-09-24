#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

/// Stress test: 5 members, 50 rounds
///
/// The tier system caps a fresh organiser's circles at 5 members
/// (`scoring::max_circle_size` = bronze tier), so "stress" is achieved by
/// sustaining a full 50-round lifecycle rather than by raw member count.
/// This test validates that the circle contract can handle repeated
/// contributions and payouts without unhandled errors.
#[test]
fn test_sustained_circle_5_members_50_rounds() {
    let env = Env::default();
    env.mock_all_auths();

    // Configure for sustained circle
    let organizer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let config = crate::types::CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(&env, "Stress Test Circle"),
        contribution_amount: 10_0000000i128, // 10 units to keep math manageable
        max_members: 5u32,
        payout_type: 1u32, // PAYOUT_FIXED for deterministic behavior
        total_rounds: 50u32,
        contribution_deadline_seconds: 604800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 86400u64,
        max_strikes: 3u32,
        slug: String::from_str(&env, "stress-test"),
    };

    let admin = organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(crate::Circle, (&admin, &factory, &config));
    let client = crate::CircleClient::new(&env, &contract_id);

    // 1. Join phase: 5 members
    let mut members: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let member = Address::generate(&env);
        client.join(&member);
        members.push_back(member);
    }

    assert_eq!(client.get_members().len(), 5);
    let status = client.get_status();
    assert_eq!(status.status, 1u32); // STATUS_ACTIVE (circle is full)
    assert_eq!(status.member_count, 5u32);

    // 2. Mint tokens for all members (enough for 50 rounds)
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    for i in 0..members.len() {
        let member = members.get(i).unwrap();
        token_client.mint(&member, &(config.contribution_amount * 50));
    }

    // 3. Execute 50 rounds
    for round in 0..50u32 {
        // All 5 members contribute
        for i in 0..members.len() {
            let member = members.get(i).unwrap();
            client.contribute(&member, &config.contribution_amount, &round);
        }

        // Trigger payout
        client.trigger_payout(&organizer, &round);

        // Verify round advanced
        let status = client.get_status();
        assert_eq!(status.current_round, round + 1);
    }

    // 4. Verify completion
    let final_status = client.get_status();
    assert_eq!(final_status.status, 2u32); // STATUS_COMPLETED
    assert_eq!(final_status.current_round, 50u32);

    // 5. Verify all members have contribution records
    let first_member = members.get(0).unwrap();
    let contributions = client.get_contributions(&first_member, &0, &100);
    assert_eq!(contributions.len(), 50); // 50 contributions
}

/// Stress test: 5 members with random payout type
/// Tests randomness resolution across multiple rounds.
#[test]
fn test_five_member_circle_random_payout() {
    let env = Env::default();
    env.mock_all_auths();

    let organizer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let config = crate::types::CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(&env, "Random Stress Test"),
        contribution_amount: 10_0000000i128,
        max_members: 5u32,
        payout_type: 0u32,   // PAYOUT_RANDOM
        total_rounds: 10u32, // Fewer rounds for random to complete reasonably
        contribution_deadline_seconds: 604800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 86400u64,
        max_strikes: 3u32,
        slug: String::from_str(&env, "random-stress"),
    };

    let admin = organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(crate::Circle, (&admin, &factory, &config));
    let client = crate::CircleClient::new(&env, &contract_id);

    // Join 5 members
    let mut members: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let member = Address::generate(&env);
        client.join(&member);
        members.push_back(member);
    }

    // Mint tokens
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    for i in 0..members.len() {
        let member = members.get(i).unwrap();
        token_client.mint(&member, &(config.contribution_amount * 10));
    }

    // Execute 10 rounds
    for round in 0..10u32 {
        for i in 0..members.len() {
            let member = members.get(i).unwrap();
            client.contribute(&member, &config.contribution_amount, &round);
        }
        client.trigger_payout(&organizer, &round);
    }

    let final_status = client.get_status();
    assert_eq!(final_status.status, 2u32); // COMPLETED
    assert_eq!(final_status.current_round, 10u32);
}

/// Stress test: Storage scaling with 5 members and 60 rounds
/// Tests long-running circles.
#[test]
fn test_storage_scaling_5_members_60_rounds() {
    let env = Env::default();
    env.mock_all_auths();

    let organizer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let config = crate::types::CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(&env, "Long Run Circle"),
        contribution_amount: 10_0000000i128,
        max_members: 5u32,
        payout_type: 1u32, // PAYOUT_FIXED
        total_rounds: 60u32,
        contribution_deadline_seconds: 604800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 86400u64,
        max_strikes: 3u32,
        slug: String::from_str(&env, "long-run"),
    };

    let admin = organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(crate::Circle, (&admin, &factory, &config));
    let client = crate::CircleClient::new(&env, &contract_id);

    // Join 5 members
    let mut members: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let member = Address::generate(&env);
        client.join(&member);
        members.push_back(member);
    }

    // Mint tokens for 60 rounds
    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    for i in 0..members.len() {
        let member = members.get(i).unwrap();
        token_client.mint(&member, &(config.contribution_amount * 60));
    }

    // Execute all 60 rounds
    for round in 0..60u32 {
        for i in 0..members.len() {
            let member = members.get(i).unwrap();
            client.contribute(&member, &config.contribution_amount, &round);
        }
        client.trigger_payout(&organizer, &round);
    }

    let final_status = client.get_status();
    assert_eq!(final_status.status, 2u32);
    assert_eq!(final_status.current_round, 60u32);

    // Verify contribution storage: 5 members * 60 rounds = 300 contribution records
    let first_member = members.get(0).unwrap();
    let contributions = client.get_contributions(&first_member, &0, &100);
    assert_eq!(contributions.len(), 60);
}

/// Edge case: Maximum member count boundary (5 members attempting to join when max is 5)
#[test]
fn test_max_member_boundary_enforcement() {
    let env = Env::default();
    env.mock_all_auths();

    let organizer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let config = crate::types::CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(&env, "Max Members Test"),
        contribution_amount: 10_0000000i128,
        max_members: 5u32,
        payout_type: 0u32,
        total_rounds: 1u32,
        contribution_deadline_seconds: 604800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 86400u64,
        max_strikes: 3u32,
        slug: String::from_str(&env, "max-test"),
    };

    let admin = organizer.clone();
    let factory = Address::generate(&env);
    let contract_id = env.register(crate::Circle, (&admin, &factory, &config));
    let client = crate::CircleClient::new(&env, &contract_id);

    // Join exactly 5 members
    for _ in 0..5 {
        let member = Address::generate(&env);
        client.join(&member);
    }

    assert_eq!(client.get_members().len(), 5);

    // Attempt to join 6th member — should fail once the circle is ACTIVE
    let extra_member = Address::generate(&env);
    let result = client.try_join(&extra_member);
    assert_eq!(result, Err(Ok(crate::CircleError::NotActive)));

    // Verify member count stayed at 5
    assert_eq!(client.get_members().len(), 5);
}
