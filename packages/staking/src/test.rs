#![cfg(test)]

use soroban_sdk::{Address, Env};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
use crate::Staking;
use crate::types::{StakingError, StakingPeriod, DataKey, UNBONDING_PERIOD_SECONDS};
use crate::StakingClient;

fn setup_test_env() -> (Env, Address, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);
    
    // Deploy a test token
    let token_contract_id = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract_id);
    
    // Mint tokens to user
    token_client.mint(&user, &1_000_000_0000); // 10,000 tokens
    
    (env, admin, user, token_contract_id)
}

fn deploy_staking_contract(env: &Env, admin: &Address, token: &Address) -> StakingClient {
    let staking_contract_id = env.register_contract(None, Staking);
    let staking_client = StakingClient::new(env, &staking_contract_id);
    
    staking_client.init(admin, token);
    
    staking_client
}

#[test]
fn test_init() {
    let (env, admin, _, token) = setup_test_env();
    
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Verify admin is stored
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    assert_eq!(stored_admin, admin);
    
    // Verify token is stored
    let stored_token: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    assert_eq!(stored_token, token);
    
    // Verify paused is false
    let paused: bool = env.storage().instance().get(&DataKey::Paused).unwrap();
    assert!(!paused);
    
    // Verify total staked is 0
    let total_staked: i128 = staking_client.get_total_staked();
    assert_eq!(total_staked, 0);
}

#[test]
fn test_stake_one_month_happy_path() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let amount = 100_0000000; // 100 tokens
    let period_months = 1; // 1 month = 1x multiplier
    
    let result = staking_client.stake(&user, &amount, &period_months);
    assert!(result.is_ok());
    
    // Verify stake position
    let stake = staking_client.get_stake(&user).unwrap();
    assert_eq!(stake.amount, amount);
    assert_eq!(stake.period, StakingPeriod::OneMonth);
    assert_eq!(stake.voting_power, amount); // 1x multiplier
    
    // Verify total staked
    let total_staked = staking_client.get_total_staked();
    assert_eq!(total_staked, amount);
    
    // Verify voting power
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, amount);
}

#[test]
fn test_stake_three_months_multiplier() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let amount = 100_0000000; // 100 tokens
    let period_months = 3; // 3 months = 2x multiplier
    
    staking_client.stake(&user, &amount, &period_months).unwrap();
    
    let stake = staking_client.get_stake(&user).unwrap();
    assert_eq!(stake.voting_power, amount * 2); // 2x multiplier
}

#[test]
fn test_stake_six_months_multiplier() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let amount = 100_0000000; // 100 tokens
    let period_months = 6; // 6 months = 3x multiplier
    
    staking_client.stake(&user, &amount, &period_months).unwrap();
    
    let stake = staking_client.get_stake(&user).unwrap();
    assert_eq!(stake.voting_power, amount * 3); // 3x multiplier
}

#[test]
fn test_stake_twelve_months_multiplier() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let amount = 100_0000000; // 100 tokens
    let period_months = 12; // 12 months = 5x multiplier
    
    staking_client.stake(&user, &amount, &period_months).unwrap();
    
    let stake = staking_client.get_stake(&user).unwrap();
    assert_eq!(stake.voting_power, amount * 5); // 5x multiplier
}

#[test]
fn test_stake_invalid_amount_zero() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let result = staking_client.stake(&user, &0, &1);
    assert_eq!(result, Err(Ok(StakingError::InvalidAmount)));
}

#[test]
fn test_stake_invalid_amount_negative() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let result = staking_client.stake(&user, &-100, &1);
    assert_eq!(result, Err(Ok(StakingError::InvalidAmount)));
}

#[test]
fn test_stake_invalid_period() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let result = staking_client.stake(&user, &100_0000000, &2); // Invalid period
    assert_eq!(result, Err(Ok(StakingError::InvalidPeriod)));
}

#[test]
fn test_stake_already_staked() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // First stake
    staking_client.stake(&user, &100_0000000, &1).unwrap();
    
    // Try to stake again
    let result = staking_client.stake(&user, &100_0000000, &1);
    assert_eq!(result, Err(Ok(StakingError::AlreadyStaked)));
}

#[test]
fn test_stake_insufficient_token_balance() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // User only has 10,000 tokens, try to stake 100,000
    let result = staking_client.stake(&user, &100_000_0000000, &1);
    assert!(result.is_err());
}

#[test]
fn test_unstake_happy_path() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Stake first
    let amount = 100_0000000;
    staking_client.stake(&user, &amount, &1).unwrap();
    
    // Unstake
    let result = staking_client.unstake(&user);
    assert!(result.is_ok());
    
    // Verify stake is removed
    assert!(staking_client.get_stake(&user).is_none());
    
    // Verify unbonding position exists
    let unbonding = staking_client.get_unbonding(&user).unwrap();
    assert_eq!(unbonding.amount, amount);
    
    // Verify total staked is reduced
    let total_staked = staking_client.get_total_staked();
    assert_eq!(total_staked, 0);
    
    // Verify voting power is 0 during unbonding
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);
}

#[test]
fn test_unstake_no_active_stake() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let result = staking_client.unstake(&user);
    assert_eq!(result, Err(Ok(StakingError::NoActiveStake)));
}

#[test]
fn test_claim_happy_path() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Stake
    let amount = 100_0000000;
    staking_client.stake(&user, &amount, &1).unwrap();
    
    // Unstake
    staking_client.unstake(&user).unwrap();
    
    // Fast-forward past unbonding period
    env.ledger().set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);
    
    // Claim
    let result = staking_client.claim(&user);
    assert!(result.is_ok());
    
    // Verify unbonding position is removed
    assert!(staking_client.get_unbonding(&user).is_none());
    
    // Verify voting power is 0
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);
}

#[test]
fn test_claim_unbonding_not_complete() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Stake
    let amount = 100_0000000;
    staking_client.stake(&user, &amount, &1).unwrap();
    
    // Unstake
    staking_client.unstake(&user).unwrap();
    
    // Try to claim immediately (unbonding not complete)
    let result = staking_client.claim(&user);
    assert_eq!(result, Err(Ok(StakingError::UnbondingNotComplete)));
}

#[test]
fn test_claim_no_unbonding_position() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let result = staking_client.claim(&user);
    assert_eq!(result, Err(Ok(StakingError::NoUnbondingPosition)));
}

#[test]
fn test_pause_contract() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Pause contract
    staking_client.pause(&admin).unwrap();
    
    // Verify paused state
    let paused: bool = env.storage().instance().get(&DataKey::Paused).unwrap();
    assert!(paused);
    
    // Try to stake while paused
    let result = staking_client.stake(&user, &100_0000000, &1);
    assert_eq!(result, Err(Ok(StakingError::ContractPaused)));
}

#[test]
fn test_unpause_contract() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Pause contract
    staking_client.pause(&admin).unwrap();
    
    // Unpause contract
    staking_client.unpause(&admin).unwrap();
    
    // Verify unpaused state
    let paused: bool = env.storage().instance().get(&DataKey::Paused).unwrap();
    assert!(!paused);
    
    // Should be able to stake now
    let result = staking_client.stake(&user, &100_0000000, &1);
    assert!(result.is_ok());
}

#[test]
fn test_pause_unauthorized() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Try to pause as non-admin
    let result = staking_client.pause(&user);
    assert_eq!(result, Err(Ok(StakingError::Unauthorized)));
}

#[test]
fn test_update_admin() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let new_admin = Address::generate(&env);
    staking_client.update_admin(&admin, &new_admin).unwrap();
    
    // Verify new admin
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    assert_eq!(stored_admin, new_admin);
    
    // Old admin should no longer be able to pause
    let result = staking_client.pause(&admin);
    assert_eq!(result, Err(Ok(StakingError::Unauthorized)));
    
    // New admin should be able to pause
    let result = staking_client.pause(&new_admin);
    assert!(result.is_ok());
}

#[test]
fn test_update_admin_unauthorized() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let new_admin = Address::generate(&env);
    
    // Try to update admin as non-admin
    let result = staking_client.update_admin(&user, &new_admin);
    assert_eq!(result, Err(Ok(StakingError::Unauthorized)));
}

#[test]
fn test_get_voting_power_no_stake() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);
}

#[test]
fn test_get_voting_power_during_unbonding() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // Stake
    staking_client.stake(&user, &100_0000000, &3).unwrap();
    
    // Unstake
    staking_client.unstake(&user).unwrap();
    
    // Voting power should be 0 during unbonding
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);
}

#[test]
fn test_full_staking_lifecycle() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    // 1. Stake
    let amount = 100_0000000;
    staking_client.stake(&user, &amount, &6).unwrap();
    
    // Verify voting power (3x multiplier for 6 months)
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, amount * 3);
    
    // 2. Unstake
    staking_client.unstake(&user).unwrap();
    
    // Verify voting power is 0
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);
    
    // 3. Fast-forward past unbonding
    env.ledger().set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);
    
    // 4. Claim
    staking_client.claim(&user).unwrap();
    
    // Verify no positions remain
    assert!(staking_client.get_stake(&user).is_none());
    assert!(staking_client.get_unbonding(&user).is_none());
}

#[test]
fn test_multiple_users_staking() {
    let (env, admin, token, _) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);
    
    // Mint tokens to users
    let token_client = TokenClient::new(&env, &token);
    token_client.mint(&user1, &1_000_0000000);
    token_client.mint(&user2, &1_000_0000000);
    token_client.mint(&user3, &1_000_0000000);
    
    // Each user stakes with different periods
    staking_client.stake(&user1, &100_0000000, &1).unwrap();  // 1x
    staking_client.stake(&user2, &100_0000000, &3).unwrap();  // 2x
    staking_client.stake(&user3, &100_0000000, &12).unwrap(); // 5x
    
    // Verify individual voting powers
    assert_eq!(staking_client.get_voting_power(&user1), 100_0000000);
    assert_eq!(staking_client.get_voting_power(&user2), 200_0000000);
    assert_eq!(staking_client.get_voting_power(&user3), 500_0000000);
    
    // Verify total staked
    assert_eq!(staking_client.get_total_staked(), 300_0000000);
}

#[test]
fn test_staking_period_as_seconds() {
    assert_eq!(StakingPeriod::OneMonth.as_seconds(), 30 * 24 * 60 * 60);
    assert_eq!(StakingPeriod::ThreeMonths.as_seconds(), 90 * 24 * 60 * 60);
    assert_eq!(StakingPeriod::SixMonths.as_seconds(), 180 * 24 * 60 * 60);
    assert_eq!(StakingPeriod::TwelveMonths.as_seconds(), 360 * 24 * 60 * 60);
}

#[test]
fn test_staking_period_from_u32() {
    assert_eq!(StakingPeriod::from_u32(1), Some(StakingPeriod::OneMonth));
    assert_eq!(StakingPeriod::from_u32(3), Some(StakingPeriod::ThreeMonths));
    assert_eq!(StakingPeriod::from_u32(6), Some(StakingPeriod::SixMonths));
    assert_eq!(StakingPeriod::from_u32(12), Some(StakingPeriod::TwelveMonths));
    assert_eq!(StakingPeriod::from_u32(2), None);
    assert_eq!(StakingPeriod::from_u32(24), None);
}
