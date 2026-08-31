use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
use common::pause;
use crate::types::{Circle, CircleConfig, Member, RoundInfo, Contribution, Dispute, Bid, Allowlist};
use crate::oracle;
use crate::payout;

#[contract]
pub struct CircleContract;

#[contractimpl]
impl CircleContract {
    pub fn init(env: Env, admin: Address, factory: Address) {
        crate::storage::set_admin(&env, &admin);
        crate::storage::set_factory(&env, &factory);
    }

    pub fn set_fee_bps(env: Env, admin: Address, fee_bps: u32) {
        admin.require_auth();
        let stored_admin = crate::storage::get_admin(&env);
        if admin != stored_admin {
            panic!("unauthorized");
        }
        if fee_bps > 10000 {
            panic!("fee_bps cannot exceed 10000");
        }
        crate::storage::set_fee_bps(&env, fee_bps);
    }

    pub fn set_treasury(env: Env, admin: Address, treasury: Address) {
        admin.require_auth();
        let stored_admin = crate::storage::get_admin(&env);
        if admin != stored_admin {
            panic!("unauthorized");
        }
        crate::storage::set_treasury(&env, &treasury);
    }

    pub fn set_reputation_registry(env: Env, admin: Address, registry: Address) {
        admin.require_auth();
        let stored_admin = crate::storage::get_admin(&env);
        if admin != stored_admin {
            panic!(
                "unauthorized"
            );
        }
        crate::storage::set_reputation_registry(&env, &registry);
    }

    pub fn deploy_circle(
        env: Env,
        admin: Address,
        config: CircleConfig,
        token: Address,
    ) -> u64 {
        admin.require_auth();
        pause::check_paused(&env);
        let circle_id = crate::storage::get_next_circle_id(&env);
        let circle = Circle {
            id: circle_id,
            admin: admin.clone(),
            config,
            token,
            state: crate::types::CircleState::Created,
            current_round: 0,
            total_rounds: 0,
        };
        crate::storage::set_circle(&env, &circle);
        crate::storage::set_next_circle_id(&env, circle_id + 1);
        circle_id
    }

    pub fn join(env: Env, member: Address) {
        member.require_auth();
        pause::check_paused(&env);
        let mut circle = crate::storage::get_circle(&env);
        if !circle.state.is_created() && !circle.state.is_funding() {
            panic!("circle not open for joining");
        }

        let allowlist = crate::storage::get_allowlist(&env);
        if !allowlist.addresses.is_empty() && !allowlist.addresses.contains(&member) {
            panic!("member not allowlisted");
        }

        let mut members = crate::storage::get_members(&env);
        if members.len() >= circle.config.max_members {
            panic!("circle is full");
        }
        for m in members.iter() {
            if m.address == member {
                panic!("already joined");
            }
        }

        let now = env.ledger().timestamp();
        members.push_back(Member {
            address: member.clone(),
            joined_at: now,
            exited_at: 0,
            status: crate::types::MemberStatus::Active,
            strikes: 0,
            position: members.len(),
        });
        crate::storage::set_members(&env, &members);

        if members.len() == circle.config.max_members {
            circle.state = crate::types::CircleState::Active;
            circle.total_rounds = circle.config.max_members as u32;
            crate::storage::set_circle(&env, &circle);
        } else if circle.state.is_created() {
            circle.state = crate::types::CircleState::Funding;
            crate::storage::set_circle(&env, &circle);
        }
    }

    pub fn contribute(env: Env, member: Address, amount: i128, round: u32) {
        member.require_auth();
        pause::check_paused(&env);
        let circle = crate::storage::get_circle(&env);
        if !circle.state.is_active() {
            panic!("circle not active");
        }
        if round != circle.current_round {
            panic!("invalid round");
        }
        if amount <= 0 || amount > circle.config.contribution_amount {
            panic!("invalid contribution amount");
        }

        let mut members = crate::storage::get_members(&env);
        let mut found = false;
        for m in members.iter() {
            if m.address == member {
                found = true;
                if m.status.is_defaulted() || m.status.is_exited() {
                    panic!("member inactive");
                }
                break;
            }
        }
        if !found {
            panic!("not a member");
        }

        let mut contributions = crate::storage::get_contributions(&env, round);
        for c in contributions.iter() {
            if c.member == member {
                panic!("already contributed for this round");
            }
        }

        let token_client = soroban_sdk::token::Client::new(&env, &circle.token);
        token_client.transfer(&member, &env.current_contract_address(), &amount);

        let now = env.ledger().timestamp();
        contributions.push_back(Contribution {
            member: member.clone(),
            amount,
            round,
            contributed_at: now,
        });
        crate::storage::set_contributions(&env, round, &contributions);
    }

    pub fn trigger_payout(env: Env, round: u32) {
        pause::check_paused(&env);
        let mut circle = crate::storage::get_circle(&env);
        if !circle.state.is_active() {
            panic!("circle not active");
        }
        if round != circle.current_round {
            panic!("invalid round");
        }

        let members = crate::storage::get_members(&env);
        let contributions = crate::storage::get_contributions(&env, round);
        if contributions.is_empty() {
            panic!("no contributions for round");
        }

        let now = env.ledger().timestamp();
        let _yield_rate_bps = oracle::get_yield_rate(&env, round).unwrap_or(0);
        let token_client = soroban_sdk::token::Client::new(&env, &circle.token);

        let recipient = payout::determine_recipient(&env, &circle, &members, &contributions, round);
        let total_pool: i128 = contributions.iter().map(|c| c.amount).sum();

        let fee_bps = crate::storage::get_fee_bps(&env);
        let fee = (total_pool * fee_bps as i128) / 10000;
        let payout_amount = total_pool - fee;

        if payout_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &recipient, &payout_amount);
        }

        if fee > 0 {
            if let Some(treasury) = crate::storage::get_treasury_opt(&env) {
                token_client.transfer(&env.current_contract_address(), &treasury, &fee);
            }
        }

        circle.current_round += 1;
        if circle.current_round >= circle.total_rounds {
            circle.state = crate::types::CircleState::Completed;
        }
        crate::storage::set_circle(&env, &circle);
    }

    pub fn exit_circle(env: Env, member: Address) {
        member.require_auth();
        pause::check_paused(&env);
        let mut members = crate::storage::get_members(&env);
        let mut found = false;
        for mut m in members.iter() {
            if m.address == member {
                found = true;
                if m.status.is_exited() {
                    panic!("already exited");
                }
                m.status = crate::types::MemberStatus::Exited;
                m.exited_at = env.ledger().timestamp();
                break;
            }
        }
        if !found {
            panic!("not a member");
        }
        crate::storage::set_members(&env, &members);
    }

    pub fn set_allowlist(env: Env, admin: Address, addresses: Vec<Address>) {
        admin.require_auth();
        let stored_admin = crate::storage::get_admin(&env);
        if admin != stored_admin {
            panic!("unauthorized");
        }
        crate::storage::set_allowlist(&env, &Allowlist { addresses });
    }

    pub fn get_allowlist(env: Env) -> Allowlist {
        crate::storage::get_allowlist(&env)
    }

    pub fn get_circle(env: Env) -> Circle {
        crate::storage::get_circle(&env)
    }

    pub fn get_members(env: Env) -> Vec<Member> {
        crate::storage::get_members(&env)
    }

    pub fn get_contributions(env: Env, round: u32) -> Vec<Contribution> {
        crate::storage::get_contributions(&env, round)
    }

    pub fn bid(env: Env, member: Address, discount_bps: u32, round: u32) {
        member.require_auth();
        pause::check_paused(&env);
        let circle = crate::storage::get_circle(&env);
        if !circle.state.is_active() {
            panic!("circle not active");
        }
        if round != circle.current_round {
            panic!("invalid round");
        }
        if discount_bps > 10000 {
            panic!("invalid discount bps");
        }
        let mut bids = crate::storage::get_bids(&env, round);
        bids.push_back(Bid {
            member,
            discount_bps,
            round,
        });
        crate::storage::set_bids(&env, round, &bids);
    }

    pub fn get_bids(env: Env, round: u32) -> Vec<Bid> {
        crate::storage::get_bids(&env, round)
    }

    pub fn vote_payout(env: Env, member: Address, candidate: Address, round: u32) {
        member.require_auth();
        pause::check_paused(&env);
        let circle = crate::storage::get_circle(&env);
        if !circle.state.is_active() {
            panic!("circle not active");
        }
        if round != circle.current_round {
            panic!("invalid round");
        }
        let mut votes = crate::storage::get_votes(&env, round);
        votes.push_back((member, candidate));
        crate::storage::set_votes(&env, round, &votes);
    }

    pub fn get_votes(env: Env, round: u32) -> Vec<(Address, Address)> {
        crate::storage::get_votes(&env, round)
    }

    pub fn raise_dispute(env: Env, complainant: Address, defendant: Address, round: u32) {
        complainant.require_auth();
        pause::check_paused(&env);
        let circle = crate::storage::get_circle(&env);
        if !circle.state.is_active() {
            panic!("circle not active");
        }
        let mut disputes = crate::storage::get_disputes(&env);
        disputes.push_back(Dispute {
            complainant,
            defendant,
            round,
            resolved: false,
        });
        crate::storage::set_disputes(&env, &disputes);
    }

    pub fn get_disputes(env: Env) -> Vec<Dispute> {
        crate::storage::get_disputes(&env)
    }

    pub fn resolve_dispute(env: Env, admin: Address, dispute_index: u32) {
        admin.require_auth();
        let stored_admin = crate::storage::get_admin(&env);
        if admin != stored_admin {
            panic!("unauthorized");
        }
        let mut disputes = crate::storage::get_disputes(&env);
        if (dispute_index as usize) >= disputes.len() as usize {
            panic!("dispute not found");
        }
        let mut dispute = disputes.get(dispute_index).unwrap();
        dispute.resolved = true;
        disputes.set(dispute_index, dispute);
        crate::storage::set_disputes(&env, &disputes);
    }

    pub fn report_late(env: Env, reporter: Address, defaulter: Address, round: u32) {
        reporter.require_auth();
        pause::check_paused(&env);
        let circle = crate::storage::get_circle(&env);
        if !circle.state.is_active() {
            panic!("circle not active");
        }
        if round != circle.current_round {
            panic!("invalid round");
        }
        let mut members = crate::storage::get_members(&env);
        let mut found = false;
        for mut m in members.iter() {
            if m.address == defaulter {
                found = true;
                m.strikes += 1;
                if m.strikes >= circle.config.max_strikes {
                    m.status = crate::types::MemberStatus::Defaulted;
                }
                break;
            }
        }
        if !found {
            panic!("member not found");
        }
        crate::storage::set_members(&env, &members);
    }
}
