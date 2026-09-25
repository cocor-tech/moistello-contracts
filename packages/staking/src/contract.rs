use soroban_sdk::{token, symbol_short, Address, Env, Vec};

use crate::types::*;
use common::pause;

/// Initialize the staking contract
pub fn init(env: &Env, admin: &Address, token: &Address) {
    admin.require_auth();
    
    // Store admin
    env.storage().instance().set(&DataKey::Admin, admin);
    
    // Store token address
    env.storage().instance().set(&DataKey::Token, token);
    
    // Initialize paused state to false
    env.storage().instance().set(&symbol_short!("paused"), &false);
    
    // Initialize total staked to 0
    env.storage().instance().set(&DataKey::TotalStaked, &0i128);

    // Initialize empty staker registry used by get_all_stakers()
    env.storage()
        .persistent()
        .set(&DataKey::StakerList, &Vec::<Address>::new(env));
}

/// Stake MOI tokens to increase governance voting power
pub fn stake(
    env: &Env,
    user: &Address,
    amount: i128,
    period_months: u32,
) -> Result<(), StakingError> {
    // Check if contract is paused
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;

    user.require_auth();

    // Validate amount
    if amount <= 0 {
        return Err(StakingError::InvalidAmount);
    }
    
    // Validate and convert period
    let period = StakingPeriod::from_u32(period_months)
        .ok_or(StakingError::InvalidPeriod)?;
    
    // Check if user already has an active stake
    if env.storage().instance().has(&DataKey::Stake(user.clone())) {
        return Err(StakingError::AlreadyStaked);
    }
    
    // Get token address
    let token_address: Address = env.storage().instance()
        .get(&DataKey::Token)
        .ok_or(StakingError::NotInitialized)?;
    
    // Transfer tokens from user to contract
    let token_client = token::Client::new(env, &token_address);
    token_client.transfer(user, &env.current_contract_address(), &amount);
    
    // Calculate voting power with multiplier
    let multiplier = period.multiplier();
    let voting_power = amount
        .checked_mul(multiplier as i128)
        .ok_or(StakingError::Overflow)?;
    
    // Calculate unlock time
    let current_time = env.ledger().timestamp();
    let unlock_time = current_time
        .checked_add(period.as_seconds())
        .ok_or(StakingError::Overflow)?;
    
    // Create stake position
    let stake_position = StakePosition {
        amount,
        period,
        start_time: current_time,
        unlock_time,
        voting_power,
    };
    
    // Store stake position
    env.storage().instance().set(&DataKey::Stake(user.clone()), &stake_position);
    
    // Update total staked
    let total_staked: i128 = env.storage().instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0);
    let new_total = total_staked
        .checked_add(amount)
        .ok_or(StakingError::Overflow)?;
    env.storage().instance().set(&DataKey::TotalStaked, &new_total);

    // Register user in the staker list (used by get_all_stakers)
    let mut stakers: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::StakerList)
        .unwrap_or_else(|| Vec::new(env));
    stakers.push_back(user.clone());
    env.storage()
        .persistent()
        .set(&DataKey::StakerList, &stakers);
    
    // Emit event
    Staked {
        user: user.clone(),
        amount,
        period: period_months,
        multiplier,
        voting_power,
    }.publish(env);
    
    Ok(())
}

/// Initiate unstake - tokens enter 14-day unbonding period
pub fn unstake(env: &Env, user: &Address) -> Result<(), StakingError> {
    // Check if contract is paused
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;
    
    user.require_auth();
    
    // Get stake position
    let stake_position: StakePosition = env.storage().instance()
        .get(&DataKey::Stake(user.clone()))
        .ok_or(StakingError::NoActiveStake)?;
    
    // Check if stake is already unlocked (optional - allow unstaking even if not unlocked)
    // If you want to enforce unlock time, uncomment:
    // let current_time = env.ledger().timestamp();
    // if current_time < stake_position.unlock_time {
    //     return Err(StakingError::StakeNotUnlocked);
    // }
    
    // Calculate unbonding times
    let current_time = env.ledger().timestamp();
    let claimable_time = current_time
        .checked_add(UNBONDING_PERIOD_SECONDS)
        .ok_or(StakingError::Overflow)?;
    
    // Create unbonding position
    let unbonding_position = UnbondingPosition {
        amount: stake_position.amount,
        unbonding_start_time: current_time,
        claimable_time,
    };
    
    // Store unbonding position
    env.storage().instance().set(&DataKey::Unbonding(user.clone()), &unbonding_position);
    
    // Remove stake position
    env.storage().instance().remove(&DataKey::Stake(user.clone()));
    
    // Remove user from staker list
    let stakers: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::StakerList)
        .unwrap_or_else(|| Vec::new(env));
    let mut updated: Vec<Address> = Vec::new(env);
    for s in stakers.iter() {
        if s != *user {
            updated.push_back(s);
        }
    }
    env.storage()
        .persistent()
        .set(&DataKey::StakerList, &updated);

    // Update total staked
    let total_staked: i128 = env.storage().instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0);
    let new_total = total_staked
        .checked_sub(stake_position.amount)
        .ok_or(StakingError::InsufficientBalance)?;
    env.storage().instance().set(&DataKey::TotalStaked, &new_total);
    
    // Emit event
    UnstakeInitiated {
        user: user.clone(),
        amount: stake_position.amount,
        claimable_time,
    }.publish(env);
    
    Ok(())
}

/// Claim tokens after unbonding period completes
pub fn claim(env: &Env, user: &Address) -> Result<(), StakingError> {
    // Check if contract is paused
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;

    user.require_auth();

    // Get unbonding position
    let unbonding_position: UnbondingPosition = env.storage().instance()
        .get(&DataKey::Unbonding(user.clone()))
        .ok_or(StakingError::NoUnbondingPosition)?;

    // Check if unbonding period is complete
    let current_time = env.ledger().timestamp();
    if current_time < unbonding_position.claimable_time {
        return Err(StakingError::UnbondingNotComplete);
    }

    // Get token address
    let token_address: Address = env.storage().instance()
        .get(&DataKey::Token)
        .ok_or(StakingError::NotInitialized)?;

    let token_client = token::Client::new(env, &token_address);

    // --- Preflight balance check (fix for #102) ---
    // Verify the contract holds enough tokens before attempting the transfer.
    // token::Client::transfer() panics on failure (Soroban host function), which
    // would produce an opaque SDK error rather than our typed StakingError.
    // By checking first we:
    //   (a) surface a machine-readable error code the Go client can classify, and
    //   (b) make the check → transfer → remove-state ordering explicit and correct.
    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance < unbonding_position.amount {
        return Err(StakingError::InsufficientContractBalance);
    }

    // Transfer tokens back to user.
    // In Soroban, if this call panics the entire transaction is rolled back, so
    // the unbonding position (written below) would never be committed.
    // We still perform the preflight above to return a typed error whenever
    // possible, and we only remove the position AFTER this line returns —
    // guaranteeing correct ordering and preventing silent fund loss.
    token_client.transfer(&env.current_contract_address(), user, &unbonding_position.amount);

    // Remove unbonding position only after the transfer has returned
    // successfully (reaching this line means the host call did not panic).
    env.storage().instance().remove(&DataKey::Unbonding(user.clone()));

    // Emit event
    Claimed {
        user: user.clone(),
        amount: unbonding_position.amount,
    }.publish(env);

    Ok(())
}

/// Get user's current voting power for governance
pub fn get_voting_power(env: &Env, user: &Address) -> i128 {
    // Check for active stake
    if let Some(stake_position) = env.storage().instance()
        .get::<DataKey, StakePosition>(&DataKey::Stake(user.clone()))
    {
        // Emit event for governance indexing
        VotingPowerQueried {
            user: user.clone(),
            voting_power: stake_position.voting_power,
        }.publish(env);
        
        return stake_position.voting_power;
    }
    
    // Check for unbonding position (no voting power during unbonding)
    if env.storage().instance().has(&DataKey::Unbonding(user.clone())) {
        VotingPowerQueried {
            user: user.clone(),
            voting_power: 0,
        }.publish(env);
        
        return 0;
    }
    
    // No stake or unbonding position
    VotingPowerQueried {
        user: user.clone(),
        voting_power: 0,
    }.publish(env);
    
    0
}

/// Get user's stake position
pub fn get_stake(env: &Env, user: &Address) -> Option<StakePosition> {
    env.storage().instance().get(&DataKey::Stake(user.clone()))
}

/// Get the raw staked token amount for a user.
///
/// Returns `0` if the user has no active stake. This is a convenience query
/// for clients that only need the amount and not the full `StakePosition`.
pub fn get_stake_amount(env: &Env, user: &Address) -> i128 {
    env.storage()
        .instance()
        .get::<DataKey, StakePosition>(&DataKey::Stake(user.clone()))
        .map(|pos| pos.amount)
        .unwrap_or(0)
}

/// Return the list of all addresses that currently hold an active stake.
///
/// The list is maintained in persistent storage: an address is added when
/// `stake()` is called and removed when `unstake()` is called.  Addresses
/// in the unbonding period are therefore not included.
///
/// Off-chain callers can use this to iterate over active stakers, e.g. for
/// governance snapshot builds or leaderboard queries.
pub fn get_all_stakers(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::StakerList)
        .unwrap_or_else(|| Vec::new(env))
}

/// Get user's unbonding position
pub fn get_unbonding(env: &Env, user: &Address) -> Option<UnbondingPosition> {
    env.storage().instance().get(&DataKey::Unbonding(user.clone()))
}

/// Get total staked amount
pub fn get_total_staked(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::TotalStaked).unwrap_or(0)
}

/// Pause the contract (admin only)
pub fn pause(env: &Env, admin: &Address) -> Result<(), StakingError> {
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    
    pause::pause(env, admin).map_err(|_| StakingError::ContractPaused)?;
    Ok(())
}

/// Unpause the contract (admin only)
pub fn unpause(env: &Env, admin: &Address) -> Result<(), StakingError> {
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    
    pause::unpause(env, admin).map_err(|_| StakingError::ContractPaused)?;
    Ok(())
}

/// Update admin (admin only)
pub fn update_admin(env: &Env, current_admin: &Address, new_admin: &Address) -> Result<(), StakingError> {
    let stored_admin: Address = env.storage().instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)?;
    
    if current_admin != &stored_admin {
        return Err(StakingError::Unauthorized);
    }
    
    current_admin.require_auth();
    env.storage().instance().set(&DataKey::Admin, new_admin);
    Ok(())
}

/// Top up an active stake position without resetting the unlock time
///
/// Merges additional tokens into the existing active stake while strictly preserving
/// the original unlock time (`unlock_time`). Updates voting power with the existing multiplier
/// and adjusts the effective start time using weighted-average accounting for rewards:
/// `effective_start = now - (initial_amount * (now - start_time)) / new_total`.
pub fn top_up(
    env: &Env,
    user: &Address,
    amount: i128,
) -> Result<(), StakingError> {
    pause::when_not_paused(env).map_err(|_| StakingError::ContractPaused)?;

    user.require_auth();

    if amount <= 0 {
        return Err(StakingError::InvalidAmount);
    }

    let stake_position: StakePosition = env
        .storage()
        .instance()
        .get(&DataKey::Stake(user.clone()))
        .ok_or(StakingError::NoActiveStake)?;

    let current_time = env.ledger().timestamp();
    // Staker is eligible to top-up up to and including the unlock time boundary
    if current_time > stake_position.unlock_time {
        return Err(StakingError::StakeNotUnlocked);
    }

    let token_address: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(StakingError::NotInitialized)?;

    let token_client = token::Client::new(env, &token_address);
    token_client.transfer(user, &env.current_contract_address(), &amount);

    let new_amount = stake_position
        .amount
        .checked_add(amount)
        .ok_or(StakingError::Overflow)?;

    let multiplier = stake_position.period.multiplier();
    let new_voting_power = new_amount
        .checked_mul(multiplier as i128)
        .ok_or(StakingError::Overflow)?;

    // Weighted-average accounting for rewards:
    // Preserves accumulated time-weight: initial_amount * (now - start_time)
    // = new_amount * (now - effective_start_time)
    let elapsed = current_time.saturating_sub(stake_position.start_time);
    let weighted_duration = if new_amount > 0 {
        ((stake_position.amount as u128).saturating_mul(elapsed as u128) / (new_amount as u128)) as u64
    } else {
        0
    };
    let effective_start_time = current_time.saturating_sub(weighted_duration);

    // CRITICAL: unlock_time remains UNCHANGED
    let unlock_time = stake_position.unlock_time;

    let updated_position = StakePosition {
        amount: new_amount,
        period: stake_position.period,
        start_time: effective_start_time,
        unlock_time,
        voting_power: new_voting_power,
    };

    env.storage()
        .instance()
        .set(&DataKey::Stake(user.clone()), &updated_position);

    let total_staked: i128 = env
        .storage()
        .instance()
        .get(&DataKey::TotalStaked)
        .unwrap_or(0);
    let new_total = total_staked
        .checked_add(amount)
        .ok_or(StakingError::Overflow)?;
    env.storage().instance().set(&DataKey::TotalStaked, &new_total);

    StakeToppedUp {
        user: user.clone(),
        added_amount: amount,
        new_total_amount: new_amount,
        unlock_time,
        new_voting_power,
    }
    .publish(env);

    Ok(())
}

/// Computes the time-weighted reward-eligible principal for a user over their active stake period.
pub fn get_time_weighted_amount(env: &Env, user: &Address) -> i128 {
    let stake_position: Option<StakePosition> = env
        .storage()
        .instance()
        .get(&DataKey::Stake(user.clone()));

    match stake_position {
        Some(pos) => {
            let current_time = env.ledger().timestamp();
            let total_span = pos.unlock_time.saturating_sub(pos.start_time);
            if total_span == 0 || current_time <= pos.start_time {
                pos.amount
            } else {
                let elapsed = current_time.saturating_sub(pos.start_time).min(total_span);
                ((pos.amount as u128).saturating_mul(elapsed as u128) / (total_span as u128)) as i128
            }
        }
        None => 0,
    }
}
