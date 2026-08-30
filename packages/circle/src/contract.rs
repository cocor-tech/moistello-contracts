use crate::oracle;
use crate::payout;
use crate::types::*;
use common::pause;
use common::reentrancy;
use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, Symbol, Val, Vec};

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn init(
        env: Env,
        admin: Address,
        factory: Address,
        token: Address,
        contribution_amount: i128,
        frequency: u64,
        max_members: u32,
        late_fee_bps: u32,
        grace_period: u64,
        max_strikes: u32,
        payout_strategy: u32,
    ) -> Result<(), CircleError> {
        if env.storage().instance().has(&DataKey::Circle) {
            return Err(CircleError::AlreadyInitialized);
        }

        admin.require_auth();

        let circle = Circle {
            admin,
            factory,
            token,
            contribution_amount,
            frequency,
            max_members,
            late_fee_bps,
            grace_period,
            max_strikes,
            payout_strategy,
            state: CircleState::Funding,
            current_round: 0,
            total_rounds: max_members,
            treasury: None,
            fee_bps: 0,
            reputation_registry: None,
            allowlist: None,
        };

        env.storage().instance().set(&DataKey::Circle, &circle);
        Ok(())
    }

    pub fn get_circle(env: Env) -> Result<Circle, CircleError> {
        env.storage()
            .instance()
            .get(&DataKey::Circle)
            .ok_or(CircleError::NotInitialized)
    }

    pub fn set_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), CircleError> {
        admin.require_auth();
        let mut circle = Self::get_circle(env.clone())?;
        if circle.admin != admin {
            return Err(CircleError::Unauthorized);
        }
        circle.treasury = Some(treasury);
        env.storage().instance().set(&DataKey::Circle, &circle);
        Ok(()) 
    }

    pub fn set_fee_bps(env: Env, admin: Address, fee_bps: u32) -> Result<(), CircleError> {
        admin.require_auth();
        let mut circle = Self::get_circle(env.clone())?;
        if circle.admin != admin {
            return Err(CircleError::Unauthorized);
        }
        if fee_bps > 10000 {
            return Err(CircleError::InvalidFeeBps);
        }
        circle.fee_bps = fee_bps;
        env.storage().instance().set(&DataKey::Circle, &circle);
        Ok(())
    }

    pub fn set_reputation_registry(
        env: Env,
        admin: Address,
        registry: Address,
    ) -> Result<(), CircleError> {
        admin.require_auth();
        let mut circle = Self::get_circle(env.clone())?;
        if circle.admin != admin {
            return Err(CircleError::Unauthorized);
        }
        circle.reputation_registry = Some(registry);
        env.storage().instance().set(&DataKey::Circle, &circle);
        Ok(())
    }

    pub fn set_allowlist(env: Env, admin: Address, allowlist: Vec<Address>) -> Result<(), CircleError> {
        admin.require_auth();
        let mut circle = Self::get_circle(env.clone())?;
        if circle.admin != admin {
            return Err(CircleError::Unauthorized);
        }
        circle.allowlist = Some(allowlist);
        env.storage().instance().set(&DataKey::Circle, &circle);
        Ok(())
    }

    pub fn get_allowlist(env: Env) -> Result<Option<Vec<Address>>, CircleError> {
        let circle = Self::get_circle(env)?;
        Ok(circle.allowlist)
    }

    pub fn join(env: Env, member: Address) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        member.require_auth();
        let mut circle = Self::get_circle(env.clone())?;

        if circle.state != CircleState::Funding {
            return Err(CircleError::NotInFundingState);
        }

        if let Some(ref allowlist) = circle.allowlist {
            if !allowlist.contains(&member) {
                return Err(CircleError::NotAllowlisted);
            }
        }

        let mut members: Vec<Member> = env
            .storage()
            .persistent()
            .get(&DataKey::Members)
            .unwrap_or_else(|| Vec::new(&env));

        if members.len() >= circle.max_members {
            return Err(CircleError::CircleFull);
        }

        for m in members.iter() {
            if m.address == member {
                return Err(CircleError::AlreadyMember);
            }
        }

        let now = env.ledger().timestamp();
        let position = members.len();
        let new_member = Member {
            address: member.clone(),
            joined_at: now,
            exited_at: 0,
            status: MemberStatus::Active,
            strikes: 0,
            position,
        };

        members.push_back(new_member);
        env.storage().persistent().set(&DataKey::Members, &members);

        if members.len() == circle.max_members {
            circle.state = CircleState::Active;
            env.storage().instance().set(&DataKey::Circle, &circle);
        }

        Ok(())
    }

    pub fn exit_circle(env: Env, member: Address) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        member.require_auth();
        let mut circle = Self::get_circle(env.clone())?;

        if circle.state != CircleState::Funding {
            return Err(CircleError::NotInFundingState);
        }

        let mut members: Vec<Member> = env
            .storage()
            .persistent()
            .get(&DataKey::Members)
            .unwrap_or_else(|| Vec::new(&env));

        let mut found = false;
        let mut updated_members = Vec::new(&env);
        for m in members.iter() {
            if m.address == member {
                found = true;
                if m.status != MemberStatus::Exited {
                    let mut exited_member = m;
                    exited_member.status = MemberStatus::Exited;
                    exited_member.exited_at = env.ledger().timestamp();
                    updated_members.push_back(exited_member);
                } else {
                    updated_members.push_back(m);
                }
            } else {
                updated_members.push_back(m);
            }
        }

        if !found {
            return Err(CircleError::NotAMember);
        }

        env.storage().persistent().set(&DataKey::Members, &updated_members);
        Ok(())
    }

    pub fn contribute(
        env: Env,
        member: Address,
        amount: i128,
        round: u32,
    ) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        member.require_auth();
        let circle = Self::get_circle(env.clone())?;

        if circle.state != CircleState::Active {
            return Err(CircleError::NotActive);
        }

        if circle.current_round != round {
            return Err(CircleError::InvalidRound);
        }

        if amount <= 0 || amount > circle.contribution_amount {
            return Err(CircleError::InvalidContributionAmount);
        }

        let members: Vec<Member> = env
            .storage()
            .persistent()
            .get(&DataKey::Members)
            .unwrap_or_else(|| Vec::new(&env));

        let mut is_member = false;
        for m in members.iter() {
            if m.address == member {
                if m.status == MemberStatus::Active {
                    is_member = true;
                }
                break;
            }
        }

        if !is_member {
            return Err(CircleError::NotAMember);
        }

        let token_client = soroban_sdk::token::Client::new(&env, &circle.token);
        token_client.transfer(&member, &env.current_contract_address(), &amount);

        let mut contributions: Vec<Contribution> = env
            .storage()
            .persistent()
            .get(&DataKey::Contributions)
            .unwrap_or_else(|| Vec::new(&env));

        let now = env.ledger().timestamp();
        let contrib = Contribution {
            round,
            member: member.clone(),
            amount,
            timestamp: now,
        };

        contributions.push_back(contrib);
        env.storage()
            .persistent()
            .set(&DataKey::Contributions, &contributions);

        Ok(())
    }

    pub fn trigger_payout(env: Env, recipient: Address, round: u32) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        let mut circle = Self::get_circle(env.clone())?;

        if circle.state != CircleState::Active {
            return Err(CircleError::NotActive);
        }

        if circle.current_round != round {
            return Err(CircleError::InvalidRound);
        }

        let members: Vec<Member> = env
            .storage()
            .persistent()
            .get(&DataKey::Members)
            .unwrap_or_else(|| Vec::new(&env));

        let contributions: Vec<Contribution> = env
            .storage()
            .persistent()
            .get(&DataKey::Contributions)
            .unwrap_or_else(|| Vec::new(&env));

        let now = env.ledger().timestamp();
        let token_client = soroban_sdk::token::Client::new(&env, &circle.token);

        let payout_amount = payout::calculate_payout(
            &env,
            &circle,
            round,
            &members,
            &contributions,
            &recipient,
        )?;

        let _yield_rate_bps = oracle::get_yield_rate(&env, round)?;

        let fee = (payout_amount * circle.fee_bps as i128) / 10000;
        let net_payout = payout_amount - fee;

        token_client.transfer(&env.current_contract_address(), &recipient, &net_payout);

        if fee > 0 {
            if let Some(ref treasury) = circle.treasury {
                let treasury_client = treasury::Client::new(&env, treasury);
                token_client.approve(&env.current_contract_address(), treasury, &fee, &(now + 1000));
                treasury_client.deposit(&circle.token, &circle.factory, &fee);
            }
        }

        circle.current_round += 1;
        if circle.current_round >= circle.total_rounds {
            circle.state = CircleState::Completed;
        }
        env.storage().instance().set(&DataKey::Circle, &circle);

        Ok(())
    }

    pub fn get_members(env: Env) -> Vec<Member> {
        env.storage()
            .persistent()
            .get(&DataKey::Members)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_contributions(env: Env) -> Vec<Contribution> {
        env.storage()
            .persistent()
            .get(&DataKey::Contributions)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), CircleError> {
        let circle = Self::get_circle(env.clone())?;
        if circle.admin != admin {
            return Err(CircleError::Unauthorized);
        }
        pause::set_paused(&env, true);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), CircleError> {
        let circle = Self::get_circle(env.clone())?;
        if circle.admin != admin {
            return Err(CircleError::Unauthorized);
        }
        pause::set_paused(&env, false);
        Ok(())
    }

    pub fn raise_dispute(
        env: Env,
        caller: Address,
        round: u32,
        reason: Symbol,
    ) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        caller.require_auth();
        let circle = Self::get_circle(env.clone())?;
        if circle.state != CircleState::Active {
            return Err(CircleError::NotActive);
        }
        let members: Vec<Member> = env
            .storage()
            .persistent()
            .get(&DataKey::Members)
            .unwrap_or_else(|| Vec::new(&env));
        let mut is_member = false;
        for m in members.iter() {
            if m.address == caller {
                is_member = true;
                break;
            }
        }
        if !is_member {
            return Err(CircleError::NotAMember);
        }

        let mut disputes: Vec<Dispute> = env
            .storage()
            .persistent()
            .get(&DataKey::Disputes)
            .unwrap_or_else(|| Vec::new(&env));

        for d in disputes.iter() {
            if d.round == round && d.complainant == caller {
                return Err(CircleError::DuplicateDispute);
            }
        }

        let dispute = Dispute {
            round,
            complainant: caller,
            reason,
            resolved: false,
        };
        disputes.push_back(dispute);
        env.storage().persistent().set(&DataKey::Disputes, &disputes);
        Ok(())
    }

    pub fn resolve_dispute(
        env: Env,
        admin: Address,
        round: u32,
        complainant: Address,
    ) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        admin.require_auth();
        let circle = Self::get_circle(env.clone())?;
        if circle.admin != admin {
            return Err(CircleError::Unauthorized);
        }

        let mut disputes: Vec<Dispute> = env
            .storage()
            .persistent()
            .get(&DataKey::Disputes)
            .unwrap_or_else(|| Vec::new(&env));

        let mut found = false;
        let mut updated = Vec::new(&env);
        for d in disputes.iter() {
            if d.round == round && d.complainant == complainant {
                found = true;
                let mut resolved_d = d;
                resolved_d.resolved = true;
                updated.push_back(resolved_d);
            } else {
                updated.push_back(d);
            }
        }

        if !found {
            return Err(CircleError::DisputeNotFound);
        }

        env.storage().persistent().set(&DataKey::Disputes, &updated);
        Ok(())
    }

    pub fn get_disputes(env: Env) -> Vec<Dispute> {
        env.storage()
            .persistent()
            .get(&DataKey::Disputes)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn submit_auction_bid(
        env: Env,
        bidder: Address,
        round: u32,
        discount_bps: u32,
    ) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        bidder.require_auth();
        let circle = Self::get_circle(env.clone())?;
        if circle.state != CircleState::Active {
            return Err(CircleError::NotActive);
        }
        if circle.payout_strategy != 1 {
            return Err(CircleError::InvalidPayoutStrategy);
        }
        if discount_bps > 10000 {
            return Err(CircleError::InvalidDiscount);
        }

        let mut bids: Vec<AuctionBid> = env
            .storage()
            .persistent()
            .get(&DataKey::Bids)
            .unwrap_or_else(|| Vec::new(&env));

        for b in bids.iter() {
            if b.round == round && b.bidder == bidder {
                return Err(CircleError::DuplicateBid);
            }
        }

        let bid = AuctionBid {
            round,
            bidder,
            discount_bps,
        };
        bids.push_back(bid);
        env.storage().persistent().set(&DataKey::Bids, &bids);
        Ok(())
    }

    pub fn get_bids(env: Env) -> Vec<AuctionBid> {
        env.storage()
            .persistent()
            .get(&DataKey::Bids)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn vote_payout(
        env: Env,
        voter: Address,
        round: u32,
        candidate: Address,
    ) -> Result<(), CircleError> {
        pause::check_paused(&env)?;
        voter.require_auth();
        let circle = Self::get_circle(env.clone())?;
        if circle.state != CircleState::Active {
            return Err(CircleError::NotActive);
        }
        if circle.payout_strategy != 2 {
            return Err(CircleError::InvalidPayoutStrategy);
        }

        let mut votes: Vec<PayoutVote> = env
            .storage()
            .persistent()
            .get(&DataKey::Votes)
            .unwrap_or_else(|| Vec::new(&env));

        for v in votes.iter() {
            if v.round == round && v.voter == voter {
                return Err(CircleError::DuplicateVote);
            }
        }

        let vote = PayoutVote {
            round,
            voter,
            candidate,
        };
        votes.push_back(vote);
        env.storage().persistent().set(&DataKey::Votes, &votes);
        Ok(())
    }

    pub fn get_votes(env: Env) -> Vec<PayoutVote> {
        env.storage()
            .persistent()
            .get(&DataKey::Votes)
            .unwrap_or_else(|| Vec::new(&env))
    }
}

mod treasury {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32v1-none/release/treasury.wasm"
    );
}
