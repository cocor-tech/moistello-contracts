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