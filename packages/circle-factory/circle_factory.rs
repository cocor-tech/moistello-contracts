// src/contracts/circle_factory.rs
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Result};
use soroban_sdk::token::Client as TokenClient;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircleConfig {
    pub organizer: Address,
    pub token: Address,
    pub contribution_amount: i128,
    pub fee_bps: u32,
}

#[contracttype]
pub enum FactoryError {
    DeploymentFailed = 1,
}

#[contracttype]
pub enum DataKey {
    Treasury,
}

#[contract]
pub struct CircleFactoryContract;

#[contractimpl]
impl CircleFactoryContract {
    pub fn deploy_circle(
        env: Env,
        config: CircleConfig,
        salt: BytesN<32>,
        deployer_wasm_hash: BytesN<32>,
    ) -> Result<Address, FactoryError> {
        config.organizer.require_auth();

        // Calculate and charge deployment fee from organizer to treasury based on fee_bps and contribution amount
        let fee_amount = (config.contribution_amount * (config.fee_bps as i128)) / 10000;
        if fee_amount > 0 {
            let treasury: Address = env
                .storage()
                .instance()
                .get(&DataKey::Treasury)
                .unwrap_or(config.organizer.clone());
            let token_client = TokenClient::new(&env, &config.token);
            token_client.transfer(&config.organizer, &treasury, &fee_amount);
        }

        // Deploy circle contract instance securely via Soroban deployer API
        let deployed_address = env
            .deployer()
            .with_address(config.organizer.clone(), salt)
            .deploy(deployer_wasm_hash);

        Ok(deployed_address)
    }
}