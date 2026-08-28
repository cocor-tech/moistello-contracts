use soroban_sdk::{symbol_short, token, Address, Env, Vec};

use crate::types::*;
use common::pause;

fn require_admin(env: &Env, admin: &Address) -> Result<(), TreasuryError> {
    let stored: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(TreasuryError::NotInitialized)?;
    if admin != &stored {
        return Err(TreasuryError::Unauthorized);
    }
    admin.require_auth();
    Ok(())
}

fn token_client<'a>(env: &'a Env) -> Result<token::Client<'a>, TreasuryError> {
    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(TreasuryError::NotInitialized)?;
    Ok(token::Client::new(env, &token))
}

pub fn init(env: &Env, admin: &Address, token: &Address) {
    admin.require_auth();
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::Token, token);
    env.storage().instance().set(&DataKey::Balance, &0i128);
    env.storage()
        .persistent()
        .set(&DataKey::Deposits, &Vec::<Deposit>::new(env));
    env.storage()
        .persistent()
        .set(&DataKey::Withdrawals, &Vec::<Withdrawal>::new(env));
}

pub fn deposit(
    env: &Env,
    from: &Address,
    amount: i128,
    circle_id: &Address,
) -> Result<(), TreasuryError> {
    pause::when_not_paused(env).map_err(|_| TreasuryError::ContractPaused)?;
    if from != circle_id {
        return Err(TreasuryError::Unauthorized);
    }
    from.require_auth();
    if amount <= 0 {
        return Err(TreasuryError::InvalidAmount);
    }

    let treasury = env.current_contract_address();
    token_client(env)?.transfer(from, &treasury, &amount);

    let bal: i128 = env.storage().instance().get(&DataKey::Balance).unwrap_or(0);
    env.storage().instance().set(
        &DataKey::Balance,
        &bal.checked_add(amount)
            .ok_or(TreasuryError::InvalidAmount)?,
    );

    let mut deps: Vec<Deposit> = env
        .storage()
        .persistent()
        .get(&DataKey::Deposits)
        .unwrap_or_else(|| Vec::new(env));
    deps.push_back(Deposit {
        from: from.clone(),
        amount,
        circle_id: circle_id.clone(),
        timestamp: env.ledger().timestamp(),
    });
    env.storage().persistent().set(&DataKey::Deposits, &deps);
    env.events().publish(
        (treasury, symbol_short!("deposit")),
        FeeDeposited {
            from: from.clone(),
            amount,
            circle_id: circle_id.clone(),
        },
    );
    Ok(())
}

pub fn withdraw(
    env: &Env,
    admin: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), TreasuryError> {
    pause::when_not_paused(env).map_err(|_| TreasuryError::ContractPaused)?;
    require_admin(env, admin)?;
    if amount <= 0 {
        return Err(TreasuryError::InvalidAmount);
    }

    let bal: i128 = env.storage().instance().get(&DataKey::Balance).unwrap_or(0);
    if bal < amount {
        return Err(TreasuryError::InsufficientBalance);
    }

    let treasury = env.current_contract_address();
    token_client(env)?.transfer(&treasury, to, &amount);

    env.storage().instance().set(
        &DataKey::Balance,
        &bal.checked_sub(amount)
            .ok_or(TreasuryError::InsufficientBalance)?,
    );

    let mut wds: Vec<Withdrawal> = env
        .storage()
        .persistent()
        .get(&DataKey::Withdrawals)
        .unwrap_or_else(|| Vec::new(env));
    wds.push_back(Withdrawal {
        admin: admin.clone(),
        to: to.clone(),
        amount,
        timestamp: env.ledger().timestamp(),
    });
    env.storage().persistent().set(&DataKey::Withdrawals, &wds);
    env.events().publish(
        (treasury, symbol_short!("withdraw")),
        FundsWithdrawn {
            to: to.clone(),
            amount,
        },
    );
    Ok(())
}

pub fn get_balance(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::Balance).unwrap_or(0)
}

pub fn get_deposits(env: &Env) -> Vec<Deposit> {
    env.storage()
        .persistent()
        .get(&DataKey::Deposits)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn rescue_tokens(
    env: &Env,
    admin: &Address,
    to: &Address,
    token: &Address,
    amount: i128,
) -> Result<(), TreasuryError> {
    require_admin(env, admin)?;
    if !pause::is_paused(env) {
        return Err(TreasuryError::ContractNotPaused);
    }
    if amount <= 0 {
        return Err(TreasuryError::InvalidAmount);
    }

    let treasury_token: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(TreasuryError::NotInitialized)?;
    if treasury_token == *token {
        let bal: i128 = env.storage().instance().get(&DataKey::Balance).unwrap_or(0);
        if bal < amount {
            return Err(TreasuryError::InsufficientBalance);
        }
        env.storage().instance().set(
            &DataKey::Balance,
            &bal.checked_sub(amount)
                .ok_or(TreasuryError::InsufficientBalance)?,
        );
    }

    let token_client = token::Client::new(env, token);
    token_client.transfer(&env.current_contract_address(), to, &amount);
    env.events().publish(
        (env.current_contract_address(), symbol_short!("rescue")),
        TokensRescued {
            to: to.clone(),
            token: token.clone(),
            amount,
        },
    );
    Ok(())
}

pub fn pause(env: &Env, a: &Address) -> Result<(), TreasuryError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(TreasuryError::NotInitialized)?;
    if a != &s {
        return Err(TreasuryError::Unauthorized);
    }
    pause::pause(env, a).map_err(|_| TreasuryError::ContractPaused)
}

pub fn unpause(env: &Env, a: &Address) -> Result<(), TreasuryError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(TreasuryError::NotInitialized)?;
    if a != &s {
        return Err(TreasuryError::Unauthorized);
    }
    pause::unpause(env, a).map_err(|_| TreasuryError::ContractPaused)
}
