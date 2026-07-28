use soroban_sdk::{Address,Env,Vec};use crate::types::*;use common::pause;
pub fn init(env:&Env,admin:&Address)->Result<(),TreasuryError>{
    admin.require_auth();
    // Prevent re-initialization.
    if env.storage().instance().has(&DataKey::Admin){return Err(TreasuryError::AlreadyInitialized);}
    // Guard against contract's own address as admin (Soroban invalid-address equivalent).
    if *admin==env.current_contract_address(){return Err(TreasuryError::InvalidAdmin);}
    env.storage().instance().set(&DataKey::Admin,admin);
    env.storage().instance().set(&DataKey::Balance,&0i128);
    env.storage().persistent().set(&DataKey::Deposits,&Vec::<Deposit>::new(env));
    env.storage().persistent().set(&DataKey::Withdrawals,&Vec::<Withdrawal>::new(env));
    Ok(())
}
pub fn deposit(env:&Env,from:&Address,amount:i128,circle_id:&Address)->Result<(),TreasuryError>{pause::when_not_paused(env).map_err(|_|TreasuryError::ContractPaused)?;from.require_auth();if amount<=0{return Err(TreasuryError::InvalidAmount);}let bal:i128=env.storage().instance().get(&DataKey::Balance).unwrap_or(0);env.storage().instance().set(&DataKey::Balance,&bal.checked_add(amount).ok_or(TreasuryError::InvalidAmount)?);let mut deps:Vec<Deposit>=env.storage().persistent().get(&DataKey::Deposits).unwrap_or_else(||Vec::new(env));deps.push_back(Deposit{from:from.clone(),amount,circle_id:circle_id.clone(),timestamp:env.ledger().timestamp()});env.storage().persistent().set(&DataKey::Deposits,&deps);FeeDeposited{from:from.clone(),amount,circle_id:circle_id.clone()}.publish(env);Ok(())}
pub fn withdraw(env:&Env,admin:&Address,to:&Address,amount:i128)->Result<(),TreasuryError>{pause::when_not_paused(env).map_err(|_|TreasuryError::ContractPaused)?;admin.require_auth();let s:Address=env.storage().instance().get(&DataKey::Admin).ok_or(TreasuryError::NotInitialized)?;if admin!=&s{return Err(TreasuryError::Unauthorized);}if amount<=0{return Err(TreasuryError::InvalidAmount);}let bal:i128=env.storage().instance().get(&DataKey::Balance).unwrap_or(0);if bal<amount{return Err(TreasuryError::InsufficientBalance);}env.storage().instance().set(&DataKey::Balance,&bal.checked_sub(amount).ok_or(TreasuryError::InsufficientBalance)?);let mut wds:Vec<Withdrawal>=env.storage().persistent().get(&DataKey::Withdrawals).unwrap_or_else(||Vec::new(env));wds.push_back(Withdrawal{admin:admin.clone(),to:to.clone(),amount,timestamp:env.ledger().timestamp()});env.storage().persistent().set(&DataKey::Withdrawals,&wds);FundsWithdrawn{to:to.clone(),amount}.publish(env);Ok(())}
pub fn get_balance(env:&Env)->i128{env.storage().instance().get(&DataKey::Balance).unwrap_or(0)}
pub fn get_deposits(env:&Env)->Vec<Deposit>{env.storage().persistent().get(&DataKey::Deposits).unwrap_or_else(||Vec::new(env))}
pub fn pause(env:&Env,a:&Address)->Result<(),TreasuryError>{let s:Address=env.storage().instance().get(&DataKey::Admin).ok_or(TreasuryError::NotInitialized)?;if a!=&s{return Err(TreasuryError::Unauthorized);}pause::pause(env,a).map_err(|_|TreasuryError::ContractPaused)}
pub fn unpause(env:&Env,a:&Address)->Result<(),TreasuryError>{let s:Address=env.storage().instance().get(&DataKey::Admin).ok_or(TreasuryError::NotInitialized)?;if a!=&s{return Err(TreasuryError::Unauthorized);}pause::unpause(env,a).map_err(|_|TreasuryError::ContractPaused)}
