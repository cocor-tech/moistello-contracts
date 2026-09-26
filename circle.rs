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

// src/contracts/circle.rs
use soroban_sdk::{contract, contractimpl, contracttype, Env};

#[contracttype]
pub enum DataKey {
    FeeBps,
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn set_fee_bps(env: Env, fee_bps: u32) {
        // FIX: Enforce strict fee basis point bounds (0 to 10,000 representing 0% to 100%)
        if fee_bps > 10000 {
            panic!("Fee basis points cannot exceed 10,000 (100%)");
        }

        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
    }

    pub fn calculate_payout_fee(env: Env, contribution_amount: i128) -> i128 {
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::FeeBps)
            .unwrap_or(0);

        if contribution_amount <= 0 || fee_bps == 0 {
            return 0;
        }

        // FIX: Use checked multiplication to prevent overflow when contribution_amount * fee_bps exceeds i128::MAX
        let amount_u128 = contribution_amount as u128;
        let fee_bps_u128 = fee_bps as u128;

        let product = amount_u128.checked_mul(fee_bps_u128).unwrap_or_else(|| {
            panic!("Overflow detected in payout fee calculation: amount and fee product exceeds u128 limit");
        });

        let fee = product / 10000u128;

        if fee > i128::MAX as u128 {
            panic!("Calculated fee exceeds maximum i128 limit");
        }

        fee as i128
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    #[should_panic(expected = "Fee basis points cannot exceed 10,000 (100%)")]
    fn test_set_fee_bps_bounds_rejection() {
        let env = Env::default();
        let contract_id = env.register(CircleContract, ());
        
        // Attempting to set fee above 10000 bps (100%) should panic
        let _ = env.invoke_contract::<()>(
            &contract_id,
            &soroban_sdk::Symbol::new(&env, "set_fee_bps"),
            soroban_sdk::vec![&env, 10001u32.into_val(&env)],
        );
    }
}

// src/contracts/circle.rs
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
pub enum DataKey {
    MemberBalance(Address),
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn auction_bid(env: Env, bidder: Address, base_amount: i128, discount_bips: u32) -> i128 {
        bidder.require_auth();

        // FIX: Cap maximum discount_bips at 5000 (50%) to prevent economic griefing and negative/zero payouts
        if discount_bips > 5000 {
            panic!("Discount basis points cannot exceed 5000 (50% maximum discount)");
        }

        if base_amount <= 0 {
            panic!("Base amount must be greater than zero");
        }

        // Calculate discounted bid amount: base_amount * (10000 - discount_bips) / 10000
        let discount_multiplier = 10000u128 - discount_bips as u128;
        let amount_u128 = base_amount as u128;

        let discounted_amount = (amount_u128 * discount_multiplier) / 10000u128;
        let final_payable = discounted_amount as i128;

        // Validate bidder has sufficient balance for the calculated discounted amount
        let balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MemberBalance(bidder.clone()))
            .unwrap_or(0);

        if balance < final_payable {
            panic!("Insufficient member balance for discounted auction bid amount");
        }

        final_payable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, Address};

    #[test]
    #[should_panic(expected = "Discount basis points cannot exceed 5000 (50% maximum discount)")]
    fn test_auction_bid_rejects_excessive_discount() {
        let env = Env::default();
        let contract_id = env.register(CircleContract, ());
        let bidder = Address::generate(&env);

        // Attempting a 100% discount (10000 bips) should be rejected
        let _ = env.invoke_contract::<i128>(
            &contract_id,
            &soroban_sdk::Symbol::new(&env, "auction_bid"),
            soroban_sdk::vec![&env, bidder.into_val(&env), 1000i128.into_val(&env), 10000u32.into_val(&env)],
        );
    }
}