use soroban_sdk::{contract, contractimpl, Address, Env, Vec, Symbol, symbol_short};
use crate::types::{Circle, CircleError, CircleStatus, Member, MemberStatus, Payout, Contribution, Bid, Vote, Dispute};
use crate::oracle;
use crate::payout;
use common::access;
use common::pause;
use common::reentrancy;
use common::upgrade;
use common::vrf;

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        contribution_amount: i128,
        collateral_amount: i128,
        contribution_deadline_seconds: u64,
        max_members: u32,
        late_fee_bps: u32,
        grace_period_hours: u32,
        max_strikes: u32,
        payout_distribution_type: u32,
        allowlist: Option<Vec<Address>>,
    ) -> Result<(), CircleError> {
        access::check_admin(&env, &admin)?;
        access::set_admin(&env, &admin)?;

        let circle = Circle {
            admin: admin.clone(),
            token,
            contribution_amount,
            collateral_amount,
            contribution_deadline_seconds,
            max_members,
            member_count: 0,
            current_round: 1,
            total_rounds: max_members,
            late_fee_bps,
            grace_period_hours,
            max_strikes,
            payout_distribution_type,
            status: CircleStatus::Created,
            created_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&Symbol::new(&env, "Circle"), &circle);
        env.storage().persistent().set(&Symbol::new(&env, "Members"), &Vec::<Member>::new(&env));
        env.storage().persistent().set(&Symbol::new(&env, "Contributions"), &Vec::<Contribution>::new(&env));
        env.storage().persistent().set(&Symbol::new(&env, "Payouts"), &Vec::<Payout>::new(&env));
        env.storage().persistent().set(&Symbol::new(&env, "Bids"), &Vec::<Bid>::new(&env));
        env.storage().persistent().set(&Symbol::new(&env, "Votes"), &Vec::<Vote>::new(&env));

        if let Some(list) = allowlist {
            env.storage().persistent().set(&Symbol::new(&env, "Allowlist"), &list);
        }

        vrf::init_vrf(&env);
        Ok(())
    }

    pub fn join(env: Env, member: Address) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        member.require_auth();

        let mut circle: Circle = env.storage().persistent().get(&Symbol::new(&env, "Circle")).ok_or(CircleError::NotInitialized)?;
        
        if circle.status != CircleStatus::Created && circle.status != CircleStatus::Funding {
            return Err(CircleError::InvalidState);
        }

        let mut members: Vec<Member> = env.storage().persistent().get(&Symbol::new(&env, "Members")).unwrap_or_else(|| Vec::new(&env));

        for m in members.iter() {
            if m.address == member {
                return Err(CircleError::AlreadyMember);
            }
        }

        if members.len() >= circle.max_members {
            return Err(CircleError::CircleFull);
        }

        if let Some(allowlist): Option<Vec<Address>> = env.storage().persistent().get(&Symbol::new(&env, "Allowlist")) {
            if !allowlist.contains(member.clone()) {
                return Err(CircleError::NotAllowlisted);
            }
        }

        circle.member_count = circle
            .member_count
            .checked_add(1)
            .ok_or(CircleError::InvalidAmount)?;

        if circle.member_count == circle.max_members {
            circle.status = CircleStatus::Active;
        } else if circle.status == CircleStatus::Created {
            circle.status = CircleStatus::Funding;
        }

        let new_member = Member {
            address: member,
            position: members.len(),
            joined_at: env.ledger().timestamp(),
            exited_at: 0,
            status: MemberStatus::Active,
            strikes: 0,
            total_contributions: 0,
            total_received: 0,
        };

        members.push_back(new_member);

        env.storage().persistent().set(&Symbol::new(&env, "Circle"), &circle);
        env.storage().persistent().set(&Symbol::new(&env, "Members"), &members);

        Ok(())
    }

    pub fn exit(env: Env, member: Address) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        member.require_auth();

        let mut circle: Circle = env.storage().persistent().get(&Symbol::new(&env, "Circle")).ok_or(CircleError::NotInitialized)?;
        
        if circle.status != CircleStatus::Created && circle.status != CircleStatus::Funding {
            return Err(CircleError::InvalidState);
        }

        let mut members: Vec<Member> = env.storage().persistent().get(&Symbol::new(&env, "Members")).unwrap_or_else(|| Vec::new(&env));
        let mut found = false;
        let mut idx = 0;

        for (i, m) in members.iter().enumerate() {
            if m.address == member {
                found = true;
                idx = i;
                break;
            }
        }

        if !found {
            return Err(CircleError::NotMember);
        }

        let mut m = members.get(idx as u32).unwrap();
        m.status = MemberStatus::Exited;
        m.exited_at = env.ledger().timestamp();
        members.set(idx as u32, m);

        if circle.member_count > 0 {
            circle.member_count -= 1;
        }

        if circle.member_count == 0 {
            circle.status = CircleStatus::Created;
        }

        env.storage().persistent().set(&Symbol::new(&env, "Circle"), &circle);
        env.storage().persistent().set(&Symbol::new(&env, "Members"), &members);

        Ok(())
    }

    pub fn contribute(env: Env, member: Address, amount: i128) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        reentrancy::check_reentrancy(&env)?;
        reentrancy::set_reentrancy(&env, true)?;

        let result = (|| {
            member.require_auth();

            let circle: Circle = env.storage().persistent().get(&Symbol::new(&env, "Circle")).ok_or(CircleError::NotInitialized)?;
            
            if circle.status != CircleStatus::Active {
                return Err(CircleError::InvalidState);
            }

            if amount != circle.contribution_amount {
                return Err(CircleError::InvalidAmount);
            }

            let members: Vec<Member> = env.storage().persistent().get(&Symbol::new(&env, "Members")).unwrap_or_else(|| Vec::new(&env));
            let mut is_member = false;
            for m in members.iter() {
                if m.address == member && m.status == MemberStatus::Active {
                    is_member = true;
                    break;
                }
            }

            if !is_member {
                return Err(CircleError::NotMember);
            }

            let token_client = soroban_token_sdk::TokenClient::new(&env, &circle.token);
            token_client.transfer(&member, &env.current_contract_address(), &amount);

            let mut contributions: Vec<Contribution> = env.storage().persistent().get(&Symbol::new(&env, "Contributions")).unwrap_or_else(|| Vec::new(&env));
            contributions.push_back(Contribution {
                member: member.clone(),
                round: circle.current_round,
                amount,
                timestamp: env.ledger().timestamp(),
            });
            env.storage().persistent().set(&Symbol::new(&env, "Contributions"), &contributions);

            Ok(())
        })();

        reentrancy::set_reentrancy(&env, false)?;
        result
    }

    pub fn trigger_payout(env: Env, recipient: Address) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        reentrancy::check_reentrancy(&env)?;
        reentrancy::set_reentrancy(&env, true)?;

        let result = payout::execute_payout(&env, recipient);

        reentrancy::set_reentrancy(&env, false)?;
        result
    }

    pub fn raise_dispute(env: Env, member: Address, description: Symbol) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        member.require_auth();

        let circle: Circle = env.storage().persistent().get(&Symbol::new(&env, "Circle")).ok_or(CircleError::NotInitialized)?;
        if circle.status != CircleStatus::Active {
            return Err(CircleError::InvalidState);
        }

        let members: Vec<Member> = env.storage().persistent().get(&Symbol::new(&env, "Members")).unwrap_or_else(|| Vec::new(&env));
        let mut is_member = false;
        for m in members.iter() {
            if m.address == member && m.status == MemberStatus::Active {
                is_member = true;
                break;
            }
        }
        if !is_member {
            return Err(CircleError::NotMember);
        }

        let mut disputes: Vec<Dispute> = env.storage().persistent().get(&Symbol::new(&env, "Disputes")).unwrap_or_else(|| Vec::new(&env));
        for d in disputes.iter() {
            if d.member == member && !d.resolved {
                return Err(CircleError::AlreadyDisputed);
            }
        }

        disputes.push_back(Dispute {
            member,
            description,
            raised_at: env.ledger().timestamp(),
            resolved: false,
        });

        env.storage().persistent().set(&Symbol::new(&env, "Disputes"), &disputes);
        Ok(())
    }

    pub fn resolve_dispute(env: Env, admin: Address, member: Address, valid: bool) -> Result<(), CircleError> {
        access::check_admin(&env, &admin)?;

        let mut disputes: Vec<Dispute> = env.storage().persistent().get(&Symbol::new(&env, "Disputes")).ok_or(CircleError::NoDispute)?;
        let mut found = false;
        let mut idx = 0;

        for (i, d) in disputes.iter().enumerate() {
            if d.member == member && !d.resolved {
                found = true;
                idx = i;
                break;
            }
        }

        if !found {
            return Err(CircleError::NoDispute);
        }

        let mut d = disputes.get(idx as u32).unwrap();
        d.resolved = true;
        disputes.set(idx as u32, d);
        env.storage().persistent().set(&Symbol::new(&env, "Disputes"), &disputes);

        if valid {
            let mut members: Vec<Member> = env.storage().persistent().get(&Symbol::new(&env, "Members")).unwrap_or_else(|| Vec::new(&env));
            for (i, m) in members.iter().enumerate() {
                if m.address == member {
                    let mut mem = m;
                    mem.strikes += 1;
                    members.set(i as u32, mem);
                    break;
                }
            }
            env.storage().persistent().set(&Symbol::new(&env, "Members"), &members);
        }

        Ok(())
    }

    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), CircleError> {
        access::check_admin_auth(&env)?;
        access::set_admin(&env, &new_admin).map_err(|_| CircleError::Unauthorized)
    }

    pub fn pause(env: Env) -> Result<(), CircleError> {
        access::check_admin_auth(&env)?;
        pause::pause_contract(&env).map_err(|_| CircleError::Unauthorized)
    }

    pub fn unpause(env: Env) -> Result<(), CircleError> {
        access::check_admin_auth(&env)?;
        pause::unpause_contract(&env).map_err(|_| CircleError::Unauthorized)
    }

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) -> Result<(), CircleError> {
        access::check_admin_auth(&env)?;
        upgrade::upgrade_contract(&env, &new_wasm_hash).map_err(|_| CircleError::Unauthorized)
    }

    pub fn get_circle(env: Env) -> Result<Circle, CircleError> {
        env.storage().persistent().get(&Symbol::new(&env, "Circle")).ok_or(CircleError::NotInitialized)
    }

    pub fn get_members(env: Env) -> Vec<Member> {
        env.storage().persistent().get(&Symbol::new(&env, "Members")).unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_contributions(env: Env) -> Vec<Contribution> {
        env.storage().persistent().get(&Symbol::new(&env, "Contributions")).unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_allowlist(env: Env) -> Option<Vec<Address>> {
        env.storage().persistent().get(&Symbol::new(&env, "Allowlist"))
    }
}
