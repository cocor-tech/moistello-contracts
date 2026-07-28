use soroban_sdk::{Address, BytesN, Env, Vec};
use crate::types::*; use common::pause;
pub fn init(env: &Env, admin: &Address, fee_bps: i128, circle_wasm_hash: &BytesN<32>) -> Result<(), FactoryError> {
    admin.require_auth();
    // Prevent re-initialization.
    if env.storage().instance().has(&DataKey::Admin) { return Err(FactoryError::AlreadyInitialized); }
    // Guard against contract's own address as admin (Soroban invalid-address equivalent).
    if *admin == env.current_contract_address() { return Err(FactoryError::InvalidAdmin); }
    if fee_bps < 0 || fee_bps > 10_000 { return Err(FactoryError::InvalidFeeBps); }
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::FeeConfig, &FeeConfig { fee_bps, updated_at: env.ledger().timestamp(), updated_by: admin.clone() });
    env.storage().instance().set(&DataKey::WasmHash, circle_wasm_hash);
    env.storage().instance().set(&DataKey::CircleCount, &0u32);
    Ok(())
}
pub fn deploy_circle(env: &Env, config: &CircleConfig) -> Result<Address, FactoryError> {
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    config.organizer.require_auth();
    if config.max_members < 2 || config.contribution_amount <= 0 || config.total_rounds == 0 || config.payout_type > 3 { return Err(FactoryError::InvalidConfig); }
    let wh: BytesN<32> = env.storage().instance().get(&DataKey::WasmHash).ok_or(FactoryError::WasmHashNotSet)?;
    let salt: BytesN<32> = env.prng().gen();
    let cid = env.deployer().with_current_contract(salt).deploy_v2(wh, ());
    let now = env.ledger().timestamp();
    let mut circles: Vec<CircleEntry> = env.storage().persistent().get(&DataKey::CircleList).unwrap_or_else(|| Vec::new(env));
    circles.push_back(CircleEntry { circle_id: cid.clone(), name: config.name.clone(), organizer: config.organizer.clone(), deployed_at: now, status: 0 });
    env.storage().persistent().set(&DataKey::CircleList, &circles);
    let c: u32 = env.storage().instance().get(&DataKey::CircleCount).unwrap_or(0);
    let next_c = c.checked_add(1).ok_or(FactoryError::InvalidConfig)?;
    env.storage().instance().set(&DataKey::CircleCount, &next_c);
    CircleDeployed { creator: config.organizer.clone(), circle_id: cid.clone(), name: config.name.clone() }.publish(env);
    Ok(cid)
}
pub fn get_circles(env: &Env) -> CircleRegistry { CircleRegistry { circles: env.storage().persistent().get(&DataKey::CircleList).unwrap_or_else(|| Vec::new(env)) } }
pub fn get_circle_count(env: &Env) -> u32 { env.storage().instance().get(&DataKey::CircleCount).unwrap_or(0) }
pub fn get_fee_config(env: &Env) -> FeeConfig { env.storage().instance().get(&DataKey::FeeConfig).unwrap_or_else(|| FeeConfig { fee_bps:0, updated_at:0, updated_by: env.current_contract_address() }) }
pub fn set_fee_config(env: &Env, admin: &Address, fee_bps: i128) -> Result<(), FactoryError> {
    pause::when_not_paused(env).map_err(|_| FactoryError::ContractPaused)?;
    admin.require_auth();
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?;
    if admin != &s { return Err(FactoryError::Unauthorized); }
    if fee_bps < 0 || fee_bps > 10_000 { return Err(FactoryError::InvalidFeeBps); }
    let old: FeeConfig = env.storage().instance().get(&DataKey::FeeConfig).unwrap_or_else(|| FeeConfig { fee_bps:0, updated_at:0, updated_by: env.current_contract_address() });
    env.storage().instance().set(&DataKey::FeeConfig, &FeeConfig { fee_bps, updated_at: env.ledger().timestamp(), updated_by: admin.clone() });
    FeeConfigUpdated { old_fee_bps: old.fee_bps, new_fee_bps: fee_bps, updated_by: admin.clone() }.publish(env);
    Ok(())
}
pub fn pause(env: &Env, admin: &Address) -> Result<(), FactoryError> { let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?; if admin != &s { return Err(FactoryError::Unauthorized); } pause::pause(env, admin).map_err(|_| FactoryError::ContractPaused) }
pub fn unpause(env: &Env, admin: &Address) -> Result<(), FactoryError> { let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?; if admin != &s { return Err(FactoryError::Unauthorized); } pause::unpause(env, admin).map_err(|_| FactoryError::ContractPaused) }
pub fn upgrade(env: &Env, admin: &Address, new_wasm_hash: &BytesN<32>) -> Result<(), FactoryError> {
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(FactoryError::NotInitialized)?;
    if admin != &s { return Err(FactoryError::Unauthorized); }
    common::upgrade::upgrade_contract(env, admin, new_wasm_hash).map_err(|_| FactoryError::Unauthorized)
}

