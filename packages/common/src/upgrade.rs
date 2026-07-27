use soroban_sdk::{Address, BytesN, Env, contracterror, symbol_short};

#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeError {
    NotAuthorized = 1,
    NotInitialized = 2,
    InvalidWasmHash = 3,
}

pub fn get_implementation(env: &Env) -> Option<Address> {
    env.storage().instance().get(&symbol_short!("impl"))
}

pub fn set_implementation(env: &Env, admin: &Address, new_impl: &Address) -> Result<(), UpgradeError> {
    admin.require_auth();
    let k = symbol_short!("impl");
    env.storage().instance().set(&k, new_impl);
    Ok(())
}

pub fn upgrade_contract(env: &Env, admin: &Address, new_wasm_hash: &BytesN<32>) -> Result<(), UpgradeError> {
    admin.require_auth();
    env.deployer().update_current_contract_wasm(new_wasm_hash.clone());
    Ok(())
}
