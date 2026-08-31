use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
use soroban_sdk::token::Client as TokenClient;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
use soroban_sdk::token::Client as TokenClient;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};
use soroban_sdk::token::Client as TokenClient;

#[contracttype]
pub enum DataKey {
    Token,
    Streak(Address),
    StreakBonusConfig,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreakConfig {
    pub base_bonus: i128,
    pub multiplier_per_day: i128,
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn claim_streak_bonus(env: Env, member: Address) {
        member.require_auth();

        // Retrieve streak count for member
        let streak_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::Streak(member.clone()))
            .unwrap_or(0);

        if streak_count == 0 {
            panic!("No active streak to claim bonus for");
        }

        // Retrieve streak bonus configuration
        let config: StreakConfig = env
            .storage()
            .instance()
            .get(&DataKey::StreakBonusConfig)
            .unwrap_or_else(|| StreakConfig {
                base_bonus: 100_0000,
                multiplier_per_day: 10_0000,
            });

        // FIX: Calculate the precise streak bonus instead of draining the entire contract balance
        let bonus_amount = config.base_bonus + (streak_count as i128) * config.multiplier_per_day;

        let token_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic!("Token not configured"));

        let token_client = TokenClient::new(&env, &token_address);

        // Verify contract has sufficient balance before transferring calculated bonus
        let contract_balance = token_client.balance(&env.current_contract_address());
        if contract_balance < bonus_amount {
            panic!("Insufficient contract balance for streak bonus payout");
        }

        // Reset streak or mark claimed to prevent double-claiming
        env.storage().persistent().set(&DataKey::Streak(member.clone()), &0u32);

        // Transfer only the exact calculated bonus amount
        token_client.transfer(
            &env.current_contract_address(),
            &member,
            &bonus_amount,
        );
    }
}

env.events().publish(
    (Symbol::new(&env, "PayoutExecuted"), round),
    PayoutExecuted {
        recipient: recipient.clone(),
        round,
        amount: net,
        distributed,
        fee,
        payout_type,
    },
);


#[contracttype]
pub enum DataKey {
    Token,
    Contribution(Address),
    ReferralConfig,
    ReferralClaimed(Address, Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferralConfig {
    pub bonus_pct: u32, // Basis points (e.g., 500 = 5%)
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn claim_referral_bonus(env: Env, referrer: Address, referred_member: Address) {
        referrer.require_auth();

        // Check if referral bonus was already claimed for this pair
        if env.storage().persistent().has(&DataKey::ReferralClaimed(referrer.clone(), referred_member.clone())) {
            panic!("Referral bonus already claimed for this member");
        }

        // Retrieve contribution amount of the referred member
        let contribution_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Contribution(referred_member.clone()))
            .unwrap_or(0);

        if contribution_amount <= 0 {
            panic!("No qualifying contribution found for referred member");
        }

        // Retrieve referral configuration
        let config: ReferralConfig = env
            .storage()
            .instance()
            .get(&DataKey::ReferralConfig)
            .unwrap_or_else(|| ReferralConfig { bonus_pct: 500 }); // Default 5%

        // FIX: Calculate precise referral bonus instead of transferring total contract balance
        let bonus_amount = (contribution_amount * (config.bonus_pct as i128)) / 10000;

        let token_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic!("Token not configured"));

        let token_client = TokenClient::new(&env, &token_address);

        // Verify contract has sufficient balance before transferring calculated bonus
        let contract_balance = token_client.balance(&env.current_contract_address());
        if contract_balance < bonus_amount {
            panic!("Insufficient contract balance for referral bonus payout");
        }

        // Mark as claimed to prevent re-entrancy and double-claiming
        env.storage().persistent().set(
            &DataKey::ReferralClaimed(referrer.clone(), referred_member.clone()),
            &true,
        );

        // Transfer only the exact calculated referral bonus amount
        token_client.transfer(
            &env.current_contract_address(),
            &referrer,
            &bonus_amount,
        );
    }
}


#[contracttype]
pub enum DataKey {
    Token,
    Streak(Address),
    StreakBonusConfig,
    StreakLastClaimedRound(Address),
    CurrentRound,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreakConfig {
    pub base_bonus: i128,
    pub multiplier_per_day: i128,
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn claim_streak_bonus(env: Env, member: Address) {
        member.require_auth();

        let streak_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::Streak(member.clone()))
            .unwrap_or(0);

        if streak_count < 3 {
            panic!("Streak count must be at least 3 to claim bonus");
        }

        let current_round: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CurrentRound)
            .unwrap_or(1);

        // FIX: Track last claimed round per member and prevent repeat claims in the same round
        let last_claimed_round: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::StreakLastClaimedRound(member.clone()))
            .unwrap_or(0);

        if last_claimed_round >= current_round {
            panic!("Streak bonus already claimed for the current round");
        }

        let config: StreakConfig = env
            .storage()
            .instance()
            .get(&DataKey::StreakBonusConfig)
            .unwrap_or_else(|| StreakConfig {
                base_bonus: 100_0000,
                multiplier_per_day: 10_0000,
            });

        let bonus_amount = config.base_bonus + (streak_count as i128) * config.multiplier_per_day;

        let token_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic!("Token not configured"));

        let token_client = TokenClient::new(&env, &token_address);

        let contract_balance = token_client.balance(&env.current_contract_address());
        if contract_balance < bonus_amount {
            panic!("Insufficient contract balance for streak bonus payout");
        }

        // Record current round as last claimed to enforce cooldown and prevent draining
        env.storage().persistent().set(
            &DataKey::StreakLastClaimedRound(member.clone()),
            &current_round,
        );

        token_client.transfer(
            &env.current_contract_address(),
            &member,
            &bonus_amount,
        );
    }
}