// src/contracts/circle.rs
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircleConfig {
    pub token: Address,
    pub base_amount: i128,
}

#[contracttype]
pub enum DataKey {
    Config,
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn initialize(env: Env, config: CircleConfig) {
        if env.storage().instance().has(&DataKey::Config) {
            panic!("Contract is already initialized");
        }

        // FIX: Validate that the token address exists on-chain and is a valid deployed contract/asset
        if !config.token.exists() {
            panic!("Invalid token address: contract or asset does not exist on-chain");
        }

        env.storage().instance().set(&DataKey::Config, &config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, Address};

    #[test]
    #[should_panic(expected = "Invalid token address: contract or asset does not exist on-chain")]
    fn test_init_rejects_non_existent_token() {
        let env = Env::default();
        let contract_id = env.register(CircleContract, ());
        
        // Generate a valid-looking dummy contract address that hasn't been deployed/created
        let fake_token = Address::generate(&env);

        let config = CircleConfig {
            token: fake_token,
            base_amount: 1000,
        };

        // Should panic because fake_token.exists() is false
        env.invoke_contract(&contract_id, &soroban_sdk::Symbol::new(&env, "initialize"), soroban_sdk::vec![&env, config.into_val(&env)]);
    }
}