use soroban_sdk::{Address, BytesN, Env, Vec, symbol_short};
use crate::types::*;
use crate::oracle;
use common::pause;

pub fn set_token(env: &Env, admin: &Address, token: &Address) -> Result<(), CircleError> {
    admin.require_auth();
    env.storage().instance().set(&DataKey::Token, token);
    Ok(()}
