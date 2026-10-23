#![cfg(test)]

use crate::types::{DataKey, StakingError, StakingPeriod, UNBONDING_PERIOD_SECONDS};
use crate::Staking;
use crate::StakingClient;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env};

fn setup_test_env() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);

    // Deploy a test token
    let token_contract_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = StellarAssetClient::new(&env, &token_contract_id);

    // Mint tokens to user
    token_admin_client.mint(&user, &1_000_000_0000); // 10,000 tokens

    (env, admin, user, token_contract_id)
}

/// Advance the ledger clock to the user's unlock boundary so lock-period
/// enforcement (#433) lets the unstake path proceed.
fn at_unlock(env: &Env, client: &StakingClient, user: &Address) {
    if let Some(stake) = client.get_stake(user) {
        env.ledger().set_timestamp(stake.unlock_time);
    }
}

fn deploy_staking_contract<'a>(
    env: &'a Env,
    admin: &Address,
    token: &Address,
) -> StakingClient<'a> {
    let staking_contract_id = env.register(Staking, ());
    let staking_client = StakingClient::new(env, &staking_contract_id);

    staking_client.init(admin, token);

    staking_client
}

#[test]
fn test_init() {
    let (env, admin, _, token) = setup_test_env();

    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Verify admin is stored
    let stored_admin: Address = env.as_contract(&staking_client.address, || {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    });
    assert_eq!(stored_admin, admin);

    // Verify token is stored
    let stored_token: Address = env.as_contract(&staking_client.address, || {
        env.storage().instance().get(&DataKey::Token).unwrap()
    });
    assert_eq!(stored_token, token);

    // Verify paused is false
    let paused: bool = env.as_contract(&staking_client.address, || {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("paused"))
            .unwrap()
    });
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

    let result = staking_client.try_stake(&user, &amount, &period_months);
    assert!(result.is_ok());

    // Verify stake position
    let stake = staking_client.get_stake(&user);
    assert_eq!(stake.as_ref().unwrap().amount, amount);
    assert_eq!(stake.as_ref().unwrap().period, StakingPeriod::OneMonth);
    assert_eq!(stake.as_ref().unwrap().voting_power, amount); // 1x multiplier

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

    staking_client.stake(&user, &amount, &period_months);

    let stake = staking_client.get_stake(&user);
    assert_eq!(stake.as_ref().unwrap().voting_power, amount * 2); // 2x multiplier
}

#[test]
fn test_stake_six_months_multiplier() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let amount = 100_0000000; // 100 tokens
    let period_months = 6; // 6 months = 3x multiplier

    staking_client.stake(&user, &amount, &period_months);

    let stake = staking_client.get_stake(&user);
    assert_eq!(stake.as_ref().unwrap().voting_power, amount * 3); // 3x multiplier
}

#[test]
fn test_stake_twelve_months_multiplier() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let amount = 100_0000000; // 100 tokens
    let period_months = 12; // 12 months = 5x multiplier

    staking_client.stake(&user, &amount, &period_months);

    let stake = staking_client.get_stake(&user);
    assert_eq!(stake.as_ref().unwrap().voting_power, amount * 5); // 5x multiplier
}

#[test]
fn test_stake_invalid_amount_zero() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let result = staking_client.try_stake(&user, &0, &1);
    assert_eq!(result, Err(Ok(StakingError::InvalidAmount)));
}

#[test]
fn test_stake_invalid_amount_negative() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let result = staking_client.try_stake(&user, &-100, &1);
    assert_eq!(result, Err(Ok(StakingError::InvalidAmount)));
}

#[test]
fn test_stake_invalid_period() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let result = staking_client.try_stake(&user, &100_0000000, &2); // Invalid period
    assert_eq!(result, Err(Ok(StakingError::InvalidPeriod)));
}

#[test]
fn test_stake_already_staked() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // First stake
    staking_client.stake(&user, &100_0000000, &1);

    // Try to stake again
    let result = staking_client.try_stake(&user, &100_0000000, &1);
    assert_eq!(result, Err(Ok(StakingError::AlreadyStaked)));
}

#[test]
fn test_stake_insufficient_token_balance() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // User only has 10,000 tokens, try to stake 100,000
    let result = staking_client.try_stake(&user, &100_000_0000000, &1);
    assert!(result.is_err());
}

#[test]
fn test_unstake_happy_path() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Stake first
    let amount = 100_0000000;
    staking_client.stake(&user, &amount, &1);
    at_unlock(&env, &staking_client, &user);

    // Unstake
    let result = staking_client.try_unstake(&user);
    assert!(result.is_ok());

    // Verify stake is removed
    assert!(staking_client.get_stake(&user).is_none());

    // Verify unbonding position exists
    let unbonding = staking_client.get_unbonding(&user);
    assert_eq!(unbonding.as_ref().unwrap().amount, amount);

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

    let result = staking_client.try_unstake(&user);
    assert_eq!(result, Err(Ok(StakingError::NoActiveStake)));
}

#[test]
fn test_claim_happy_path() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Stake
    let amount = 100_0000000;
    staking_client.stake(&user, &amount, &1);
    at_unlock(&env, &staking_client, &user);

    // Unstake
    staking_client.unstake(&user);

    // Fast-forward past unbonding period
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);

    // Claim
    let result = staking_client.try_claim(&user);
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
    staking_client.stake(&user, &amount, &1);
    at_unlock(&env, &staking_client, &user);

    // Unstake
    staking_client.unstake(&user);

    // Try to claim immediately (unbonding not complete)
    let result = staking_client.try_claim(&user);
    assert_eq!(result, Err(Ok(StakingError::UnbondingNotComplete)));
}

#[test]
fn test_claim_no_unbonding_position() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let result = staking_client.try_claim(&user);
    assert_eq!(result, Err(Ok(StakingError::NoUnbondingPosition)));
}

#[test]
fn test_pause_contract() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Pause contract
    staking_client.pause(&admin);

    // Verify paused state
    let paused: bool = env.as_contract(&staking_client.address, || {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("paused"))
            .unwrap()
    });
    assert!(paused);

    // Try to stake while paused
    let result = staking_client.try_stake(&user, &100_0000000, &1);
    assert_eq!(result, Err(Ok(StakingError::ContractPaused)));
}

#[test]
fn test_unpause_contract() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Pause contract
    staking_client.pause(&admin);

    // Unpause contract
    staking_client.unpause(&admin);

    // Verify unpaused state
    let paused: bool = env.as_contract(&staking_client.address, || {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("paused"))
            .unwrap()
    });
    assert!(!paused);

    // Should be able to stake now
    let result = staking_client.try_stake(&user, &100_0000000, &1);
    assert!(result.is_ok());
}

#[test]
fn test_pause_blocks_admin_and_slash_mutations() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    let new_admin = Address::generate(&env);

    // Pause contract
    staking_client.pause(&admin);

    // All admin-gated mutations are rejected while paused.
    assert_eq!(
        staking_client.try_update_admin(&admin, &new_admin),
        Err(Ok(StakingError::ContractPaused))
    );
    assert_eq!(
        staking_client.try_slash(&admin, &user, &100),
        Err(Ok(StakingError::ContractPaused))
    );
    assert_eq!(
        staking_client.try_set_slash_notice_period(&admin, &3600),
        Err(Ok(StakingError::ContractPaused))
    );
    assert_eq!(
        staking_client.try_create_slash_notice(&admin, &user, &100),
        Err(Ok(StakingError::ContractPaused))
    );
    assert_eq!(
        staking_client.try_cancel_slash(&admin, &user),
        Err(Ok(StakingError::ContractPaused))
    );
    assert_eq!(
        staking_client.try_execute_slash(&admin, &user),
        Err(Ok(StakingError::ContractPaused))
    );
    // User-facing mutations are rejected too.
    assert_eq!(staking_client.try_top_up(&user, &100), Err(Ok(StakingError::ContractPaused)));

    // Unpause and verify admin mutation resumes.
    staking_client.unpause(&adm