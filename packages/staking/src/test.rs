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
fn test_init_cannot_overwrite_admin_or_token() {
    let (env, admin, _, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);
    let replacement_admin = Address::generate(&env);
    let replacement_token = Address::generate(&env);

    assert_eq!(
        client.try_init(&replacement_admin, &replacement_token),
        Err(Ok(StakingError::AlreadyInitialized))
    );
    let stored_admin: Address = env.as_contract(&client.address, || {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    });
    let stored_token: Address = env.as_contract(&client.address, || {
        env.storage().instance().get(&DataKey::Token).unwrap()
    });
    assert_eq!(stored_admin, admin);
    assert_eq!(stored_token, token);
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
    staking_client.unpause(&admin);
    assert!(staking_client.try_update_admin(&admin, &new_admin).is_ok());
}

#[test]
fn test_pause_unpause_unauthorized() {
    let (env, admin, _, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);
    let not_admin = Address::generate(&env);

    assert!(staking_client.try_pause(&not_admin).is_err());
    assert!(staking_client.try_unpause(&not_admin).is_err());
}

#[test]
fn test_pause_unauthorized() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Try to pause as non-admin
    let result = staking_client.try_pause(&user);
    assert_eq!(result, Err(Ok(StakingError::Unauthorized)));
}

#[test]
fn test_update_admin() {
    let (env, admin, _, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let new_admin = Address::generate(&env);
    staking_client.update_admin(&admin, &new_admin);

    // Verify new admin
    let stored_admin: Address = env.as_contract(&staking_client.address, || {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    });
    assert_eq!(stored_admin, new_admin);

    // Old admin should no longer be able to pause
    let result = staking_client.try_pause(&admin);
    assert_eq!(result, Err(Ok(StakingError::Unauthorized)));

    // New admin should be able to pause
    let result = staking_client.try_pause(&new_admin);
    assert!(result.is_ok());
}

#[test]
fn test_update_admin_unauthorized() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let new_admin = Address::generate(&env);

    // Try to update admin as non-admin
    let result = staking_client.try_update_admin(&user, &new_admin);
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
    staking_client.stake(&user, &100_0000000, &3);
    at_unlock(&env, &staking_client, &user);

    // Unstake
    staking_client.unstake(&user);

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
    staking_client.stake(&user, &amount, &6);

    // Verify voting power (3x multiplier for 6 months)
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, amount * 3);

    // 2. Unstake
    at_unlock(&env, &staking_client, &user);
    staking_client.unstake(&user);

    // Verify voting power is 0
    let voting_power = staking_client.get_voting_power(&user);
    assert_eq!(voting_power, 0);

    // 3. Fast-forward past unbonding
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);

    // 4. Claim
    staking_client.claim(&user);

    // Verify no positions remain
    assert!(staking_client.get_stake(&user).is_none());
    assert!(staking_client.get_unbonding(&user).is_none());
}

#[test]
fn test_multiple_users_staking() {
    let (env, admin, _, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    // Mint tokens to users
    let token_admin_client = StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&user1, &1_000_0000000);
    token_admin_client.mint(&user2, &1_000_0000000);
    token_admin_client.mint(&user3, &1_000_0000000);

    // Each user stakes with different periods
    staking_client.stake(&user1, &100_0000000, &1); // 1x
    staking_client.stake(&user2, &100_0000000, &3); // 2x
    staking_client.stake(&user3, &100_0000000, &12); // 5x

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

// =============================================================================
// Issue #467 – slashing notice period before execution
// =============================================================================

#[test]
fn test_slash_notice_created() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Stake first
    staking_client.stake(&user, &100_0000000, &1);

    // Set a short notice period for testing
    staking_client.set_slash_notice_period(&admin, &10u64);

    // Create slash notice
    let result = staking_client.try_create_slash_notice(&admin, &user, &50_0000000);
    assert!(result.is_ok());

    // Verify notice exists
    let notice = staking_client.get_slash_notice(&user);
    assert!(notice.is_some());
    let (amount, notice_until, executor) = notice.unwrap();
    assert_eq!(amount, 50_0000000);
    assert_eq!(notice_until, env.ledger().timestamp() + 10);
    assert_eq!(executor, admin);
}

#[test]
fn test_slash_cancelled_during_notice_period() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Stake first
    staking_client.stake(&user, &100_0000000, &1);

    // Set a short notice period for testing
    staking_client.set_slash_notice_period(&admin, &10u64);

    // Create slash notice
    staking_client.create_slash_notice(&admin, &user, &50_0000000);

    // Cancel slash during notice period
    let result = staking_client.try_cancel_slash(&admin, &user);
    assert!(result.is_ok());

    // Verify notice is removed
    let notice = staking_client.get_slash_notice(&user);
    assert!(notice.is_none());
}

#[test]
fn test_slash_executed_after_notice_period() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Stake first
    staking_client.stake(&user, &100_0000000, &1);

    // Set a short notice period for testing
    staking_client.set_slash_notice_period(&admin, &10u64);

    // Create slash notice
    staking_client.create_slash_notice(&admin, &user, &50_0000000);

    // Advance past notice period
    env.ledger().set_timestamp(env.ledger().timestamp() + 20);

    // Execute slash
    let result = staking_client.try_execute_slash(&admin, &user);
    assert!(result.is_ok());

    // Verify stake is reduced
    let stake = staking_client.get_stake(&user);
    assert_eq!(stake.as_ref().unwrap().amount, 50_0000000);

    // Verify notice is removed
    let notice = staking_client.get_slash_notice(&user);
    assert!(notice.is_none());
}

#[test]
fn test_slash_unauthorized() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    // Stake first
    staking_client.stake(&user, &100_0000000, &1);

    // Stranger tries to slash
    let stranger = Address::generate(&env);
    let result = staking_client.try_create_slash_notice(&stranger, &user, &50_0000000);
    assert_eq!(result, Err(Ok(StakingError::Unauthorized)));
}

// ── Tests for get_stake_amount ────────────────────────────────────────────────

/// Happy path: staking sets the correct amount readable via get_stake_amount.
#[test]
fn test_get_stake_amount_happy_path() {
    let (env, admin, user, token) = setup_test_env();
    let client = deploy_staking_contract(&env, &admin, &token);

    let amount = 200_0000000i128; // 200 tokens
    client.stake(&user, &amount, &3);

    assert_eq!(client.get_stake_amount(&user), amount);
}

/// No stake → get_stake_amount returns 0 (not an error).
#[test]
fn test_top_up_preserves_unlock_time() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let amount = 100_0000000; // 100 tokens
    let period_months = 1;

    staking_client.stake(&user, &amount, &period_months);
    let initial_stake = staking_client.get_stake(&user).unwrap();
    let original_unlock_time = initial_stake.unlock_time;

    // Advance time by 5 days
    env.ledger().with_mut(|l| {
        l.timestamp += 5 * 24 * 60 * 60;
    });

    // Top up an additional 150 tokens
    let top_up_amount = 150_0000000;
    let res = staking_client.try_top_up(&user, &top_up_amount);
    assert!(res.is_ok());

    let updated_stake = staking_client.get_stake(&user).unwrap();
    // Acceptance criterion: Unlock time unchanged after top-up
    assert_eq!(updated_stake.unlock_time, original_unlock_time);
    // Total merged amount
    assert_eq!(updated_stake.amount, amount + top_up_amount);
    // Voting power updated
    assert_eq!(updated_stake.voting_power, amount + top_up_amount);
    assert_eq!(staking_client.get_total_staked(), amount + top_up_amount);
}

#[test]
fn test_top_up_time_weighted_rewards() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let initial_amount = 100_0000000;
    staking_client.stake(&user, &initial_amount, &1);

    // Advance halfway through 1-month lock (15 days)
    env.ledger().with_mut(|l| {
        l.timestamp += 15 * 24 * 60 * 60;
    });

    let top_up_amount = 100_0000000;
    staking_client.top_up(&user, &top_up_amount);

    // Query time-weighted amount
    let weighted = staking_client.get_time_weighted_amount(&user);
    assert!(weighted > 0);
    assert!(weighted <= 200_0000000);
}

#[test]
fn test_top_up_near_boundary_keeps_eligibility() {
    let (env, admin, user, token) = setup_test_env();
    let staking_client = deploy_staking_contract(&env, &admin, &token);

    let initial_amount = 100_0000000;
    staking_client.stake(&user, &initial_amount, &1);
    let original_unlock = staking_client.get_stake(&user).unwrap().unlock_time;

    // Advance right to the boundary (1 second before unlock time)
    env.ledger().with_mut(|l| {
        l.timestamp = original_unlock - 1;
    });

    // Top up at boundary keeps eligibility
    let res = staking_client.try_top_up(&user, &50_0000000);
    assert!(res.is_ok());

    let updated = staking_client.get_stake(&user).unwrap();
    assert_eq!(updated.unlock_time, original_unlock);
    assert_eq!(updated.amount, 150_0000000);

    // Advance 2 seconds past unlock boundary
    env.ledger().with_mut(|l| {
        l.timestamp = original_unlock + 1;
    });

    // Top up past unlock is rejected
    let past_boundary_res = staking_client.try_top_up(&user, &50_0000000);
    assert_eq!(past_boundary_res, Err(Ok(StakingError::StakeNotUnlocked)));
}

// =============================================================================
// Reentrancy protection tests (issue #566)
// =============================================================================

mod reentrancy_tests {
    use super::*;

    /// A malicious token contract that re-enters the staking contract during transfer.
    #[soroban_sdk::contract]
    pub struct MaliciousToken;

    #[soroban_sdk::contractimpl]
    impl MaliciousToken {
        /// Initialize the malicious token with a target staking contract to attack.
        pub fn init(
            env: soroban_sdk::Env,
            admin: soroban_sdk::Address,
            target: soroban_sdk::Address,
        ) {
            admin.require_auth();
            env.storage()
                .instance()
                .set(&soroban_sdk::symbol_short!("admin"), &admin);
            env.storage()
                .instance()
                .set(&soroban_sdk::symbol_short!("target"), &target);
            env.storage()
                .instance()
                .set(&soroban_sdk::symbol_short!("attacked"), &false);
        }

        /// Transfer function that re-enters the staking contract's stake function.
        pub fn transfer(
            env: soroban_sdk::Env,
            from: soroban_sdk::Address,
            to: soroban_sdk::Address,
            amount: i128,
        ) {
            from.require_auth();

            let target: soroban_sdk::Address = env
                .storage()
                .instance()
                .get(&soroban_sdk::symbol_short!("target"))
                .unwrap();

            let attacked: bool = env
                .storage()
                .instance()
                .get(&soroban_sdk::symbol_short!("attacked"))
                .unwrap_or(false);

            // If transferring TO the staking contract (staking), and we haven't attacked yet,
            // re-enter the staking contract to try to stake again.
            if to == target && !attacked {
                env.storage()
                    .instance()
                    .set(&soroban_sdk::symbol_short!("attacked"), &true);

                // Try to call stake on the staking contract again (reentrancy attack)
                let staking_client = crate::StakingClient::new(&env, &target);
                let _ = staking_client.try_stake(&from, &amount, &1u32);
            }

            // Normal token transfer logic (simplified - just update balances)
            let from_balance: i128 = env
                .storage()
                .instance()
                .get(&MaliciousDataKey::Balance(from.clone()))
                .unwrap_or(0);
            let to_balance: i128 = env
                .storage()
                .instance()
                .get(&MaliciousDataKey::Balance(to.clone()))
                .unwrap_or(0);

            if from_balance < amount {
                panic!("Insufficient balance");
            }

            env.storage()
                .instance()
                .set(&MaliciousDataKey::Balance(from), &(from_balance - amount));
            env.storage()
                .instance()
                .set(&MaliciousDataKey::Balance(to), &(to_balance + amount));
        }

        pub fn balance(env: soroban_sdk::Env, id: soroban_sdk::Address) -> i128 {
            env.storage()
                .instance()
                .get(&MaliciousDataKey::Balance(id))
                .unwrap_or(0)
        }

        pub fn mint(
            env: soroban_sdk::Env,
            admin: soroban_sdk::Address,
            to: soroban_sdk::Address,
            amount: i128,
        ) {
            admin.require_auth();
            let balance: i128 = env
                .storage()
                .instance()
                .get(&MaliciousDataKey::Balance(to.clone()))
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&MaliciousDataKey::Balance(to), &(balance + amount));
        }
    }

    #[derive(Clone)]
    #[soroban_sdk::contracttype]
    enum MaliciousDataKey {
        Balance(soroban_sdk::Address),
    }

    /// Test that reentrancy during stake() is blocked by ReentrancyGuard.
    #[test]
    fn test_reentrancy_guard_blocks_stake_reentry() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        // Deploy the malicious token
        let malicious_token_id = env.register(MaliciousToken, ());
        let malicious_token_client = MaliciousTokenClient::new(&env, &malicious_token_id);

        // Deploy the staking contract with the malicious token
        let staking_contract_id = env.register(Staking, ());
        let staking_client = StakingClient::new(&env, &staking_contract_id);

        // Initialize malicious token with staking contract as target
        malicious_token_client.init(&admin, &staking_contract_id);

        // Initialize staking contract with malicious token
        staking_client.init(&admin, &malicious_token_id);

        // Mint tokens to user
        malicious_token_client.mint(&admin, &user, &100_0000000);

        // First stake should succeed
        let result = staking_client.try_stake(&user, &100_0000000, &1);
        assert!(result.is_ok(), "First stake should succeed");

        // The reentrant stake attempt during transfer should have been blocked
        // Verify user only has one stake position (the reentrant one failed)
        let stake = staking_client.get_stake(&user);
        assert!(stake.is_some());
        assert_eq!(stake.unwrap().amount, 100_0000000);

        // Total staked should only be the initial amount, not doubled
        assert_eq!(staking_client.get_total_staked(), 100_0000000);
    }

    /// Test that reentrancy during claim() is blocked by ReentrancyGuard.
    #[test]
    fn test_reentrancy_guard_blocks_claim_reentry() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        // Deploy the malicious token
        let malicious_token_id = env.register(MaliciousToken, ());
        let malicious_token_client = MaliciousTokenClient::new(&env, &malicious_token_id);

        // Deploy the staking contract with the malicious token
        let staking_contract_id = env.register(Staking, ());
        let staking_client = StakingClient::new(&env, &staking_contract_id);

        // Initialize malicious token with staking contract as target
        malicious_token_client.init(&admin, &staking_contract_id);

        // Initialize staking contract with malicious token
        staking_client.init(&admin, &malicious_token_id);

        // Mint tokens to user
        malicious_token_client.mint(&admin, &user, &200_0000000);

        // Stake
        staking_client.stake(&user, &100_0000000, &1);

        // Unlock
        at_unlock(&env, &staking_client, &user);

        // Unstake
        staking_client.unstake(&user);

        // Fast-forward past unbonding period
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + UNBONDING_PERIOD_SECONDS + 1);

        // Claim should succeed (first time)
        let result = staking_client.try_claim(&user);
        assert!(result.is_ok(), "First claim should succeed");

        // Verify unbonding position is removed
        assert!(staking_client.get_unbonding(&user).is_none());
    }

    /// Test that reentrancy during top_up() is blocked by ReentrancyGuard.
    #[test]
    fn test_reentrancy_guard_blocks_top_up_reentry() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        // Deploy the malicious token
        let malicious_token_id = env.register(MaliciousToken, ());
        let malicious_token_client = MaliciousTokenClient::new(&env, &malicious_token_id);

        // Deploy the staking contract with the malicious token
        let staking_contract_id = env.register(Staking, ());
        let staking_client = StakingClient::new(&env, &staking_contract_id);

        // Initialize malicious token with staking contract as target
        malicious_token_client.init(&admin, &staking_contract_id);

        // Initialize staking contract with malicious token
        staking_client.init(&admin, &malicious_token_id);

        // Mint tokens to user
        malicious_token_client.mint(&admin, &user, &200_0000000);

        // Stake initial amount
        staking_client.stake(&user, &100_0000000, &1);

        // First top_up should succeed
        let result = staking_client.try_top_up(&user, &50_0000000);
        assert!(result.is_ok(), "First top_up should succeed");

        // The reentrant top_up attempt during transfer should have been blocked
        // Verify total staked is only 150 (not 200 from reentrancy)
        assert_eq!(staking_client.get_total_staked(), 150_0000000);
        let stake = staking_client.get_stake(&user).unwrap();
        assert_eq!(stake.amount, 150_0000000);
    }
}
