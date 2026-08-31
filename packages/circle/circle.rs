// src/contracts/circle.rs
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