use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

use crate::types::{Circle, CircleError, Member, MemberStatus, Payout, Contribution, Bid, Vote, Dispute};

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn init(
        env: Env,
        admin: Address,
        token: Address,
        contribution_amount: i128,
        contribution_deadline_seconds: u64,
        max_members: u32,
        payout_strategy: u32,
        late_fee_bps: u32,
        grace_period_hours: u32,
        max_strikes: u32,
        collateral_amount: i128,
    ) -> Result<(), CircleError> {
        if env.storage().instance().has(&soroban_sdk::symbol_short!("Admin")) {
            return Err(CircleError::AlreadyInitialized);
        }

        env.storage().instance().set(&soroban_sdk::symbol_short!("Admin"), &admin);
        env.storage().instance().set(&soroban_sdk::symbol_short!("Token"), &token);

        let circle = Circle {
            admin,
            token,
            contribution_amount,
            contribution_deadline_seconds,
            max_members,
            payout_strategy,
            late_fee_bps,
            grace_period_hours,
            max_strikes,
            collateral_amount,
            member_count: 0,
            current_round: 0,
            status: 0,
            created_at: env.ledger().timestamp(),
        };

        env.storage().instance().set(&soroban_sdk::symbol_short!("Circle"), &circle);

        let members: Vec<Member> = Vec::new(&env);
        env.storage().persistent().set(&soroban_sdk::symbol_short!("Members"), &members);

        let contributions: Vec<Contribution> = Vec::new(&env);
        env.storage().persistent().set(&soroban_sdk::symbol_short!("Contributions"), &contributions);

        let payouts: Vec<Payout> = Vec::new(&env);
        env.storage().persistent().set(&soroban_sdk::symbol_short!("Payouts"), &payouts);

        let bids: Vec<Bid> = Vec::new(&env);
        env.storage().persistent().set(&soroban_sdk::symbol_short!("Bids"), &bids);

        let votes: Vec<Vote> = Vec::new(&env);
        env.storage().persistent().set(&soroban_sdk::symbol_short!("Votes"), &votes);

        Ok(())
    }

    pub fn join(env: Env, member_address: Address) -> Result<(), CircleError> {
        member_address.require_auth();

        let mut circle: Circle = env
            .storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("Circle"))
            .ok_or(CircleError::NotInitialized)?;

        if circle.member_count >= circle.max_members {
            return Err(CircleError::CircleFull);
        }

        let mut members: Vec<Member> = env
            .storage()
            .persistent()
            .get(&soroban_sdk::symbol_short!("Members"))
            .unwrap_or_else(|| Vec::new(&env));

        for m in members.iter() {
            if m.address == member_address {
                return Err(CircleError::AlreadyJoined);
            }
        }

        let new_member = Member {
            address: member_address,
            joined_at: env.ledger().timestamp(),
            exited_at: 0,
            position: circle.member_count,
            status: MemberStatus::Active as u32,
            strikes: 0,
            total_contributions: 0,
            total_received: 0,
        };

        members.push_back(new_member);
        env.storage().persistent().set(&soroban_sdk::symbol_short!("Members"), &members);

        circle.member_count = circle
            .member_count
            .checked_add(1)
            .ok_or(CircleError::InvalidAmount)?;

        env.storage().instance().set(&soroban_sdk::symbol_short!("Circle"), &circle);

        Ok(())
    }

    pub fn get_circle(env: Env) -> Result<Circle, CircleError> {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("Circle"))
            .ok_or(CircleError::NotInitialized)
    }

    pub fn get_members(env: Env) -> Vec<Member> {
        env.storage()
            .persistent()
            .get(&soroban_sdk::symbol_short!("Members"))
            .unwrap_or_else(|| Vec::new(&env))
    }
}
