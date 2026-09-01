use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec, Symbol};
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
pub enum DataKey {
    Referral(Address),
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn register_referral(env: Env, member: Address, referrer: Address) {
        member.require_auth();

        // FIX: Enforce strict on-chain check to prevent self-referral
        if member == referrer {
            panic!("Self-referral is prohibited: referrer cannot match member address");
        }

        if env.storage().persistent().has(&DataKey::Referral(member.clone())) {
            panic!("Member has already been referred");
        }

        env.storage().persistent().set(&DataKey::Referral(member.clone()), &referrer);

        env.events().publish(
            (Symbol::new(&env, "ReferralRegistered"), member.clone()),
            referrer,
        );
    }
}


#[contracttype]
pub enum DataKey {
    Creator,
    MemberJoined(Address),
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn batch_invite(env: Env, creator: Address, members: Vec<Address>) {
        creator.require_auth();

        let stored_creator: Address = env
            .storage()
            .instance()
            .get(&DataKey::Creator)
            .unwrap_or_else(|| panic!("Creator not configured"));

        if creator != stored_creator {
            panic!("Only the circle creator can batch invite members");
        }

        if members.is_empty() {
            panic!("Member list cannot be empty");
        }

        if members.len() > 100 {
            panic!("Batch invite size exceeds maximum limit of 100 members");
        }

        for member in members.iter() {
            // Check if member is already joined to prevent duplicate registrations
            if env.storage().persistent().has(&DataKey::MemberJoined(member.clone())) {
                continue;
            }

            env.storage().persistent().set(&DataKey::MemberJoined(member.clone()), &true);

            env.events().publish(
                (Symbol::new(&env, "MemberJoined"), member.clone()),
                creator.clone(),
            );
        }
    }
}


#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contribution {
    pub amount: i128,
    pub timestamp: u64,
}

#[contracttype]
pub enum DataKey {
    Contribution(Address),
}

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn contribute(env: Env, member: Address, amount: i128) {
        member.require_auth();

        if amount <= 0 {
            panic!("Contribution amount must be greater than zero");
        }

        let timestamp = env.ledger().timestamp();

        // FIX/FEATURE: Store contribution with ledger timestamp for time-weighted calculations
        let contribution = Contribution { amount, timestamp };
        env.storage().persistent().set(&DataKey::Contribution(member.clone()), &contribution);
    }

    pub fn calculate_time_weighted_share(env: Env, member: Address) -> i128 {
        let contribution: Contribution = env
            .storage()
            .persistent()
            .get(&DataKey::Contribution(member.clone()))
            .unwrap_or(Contribution { amount: 0, timestamp: 0 });

        if contribution.amount == 0 {
            return 0;
        }

        let current_time = env.ledger().timestamp();
        if current_time <= contribution.timestamp {
            return contribution.amount; // Default to base amount if within same ledger timestamp
        }

        let time_held = (current_time - contribution.timestamp) as i128;
        
        // Time-weighted score calculation: amount * time_held (seconds)
        contribution.amount * time_held
    }
}