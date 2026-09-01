use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, Symbol, Val, Vec};
use crate::types::CircleError;

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn init(env: Env, admin: Address, treasury: Address) -> Result<(), CircleError> {
        admin.require_auth();
        Ok(())
    }

    pub fn set_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), CircleError> {
        admin.require_auth();
        Ok(())
    }

    pub fn set_treasury(env: Env, admin: Address, new_treasury: Address) -> Result<(), CircleError> {
        admin.require_auth();
        Ok(())
    }

    pub fn set_token(env: &Env, admin: &Address, token: &Address) -> Result<(), CircleError> {
        admin.require_auth();
        Ok(())
    }
}
