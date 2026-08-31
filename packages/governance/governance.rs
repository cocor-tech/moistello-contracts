// src/contracts/governance.rs
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};
use soroban_sdk::token::Client as TokenClient;

#[contracttype]
pub enum DataKey {
    GovernanceToken,
    Deposit(u64),
    Proposer(u64),
    ProposalStatus(u64),
}

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    pub fn create_proposal(
        env: Env,
        proposer: Address,
        proposal_id: u64,
        deposit_amount: i128,
    ) {
        proposer.require_auth();

        if deposit_amount <= 0 {
            panic!("Deposit amount must be positive");
        }

        if env.storage().persistent().has(&DataKey::Deposit(proposal_id)) {
            panic!("Proposal already exists");
        }

        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::GovernanceToken)
            .unwrap_or_else(|| panic!("Governance token not configured"));
        
        let token_client = TokenClient::new(&env, &token);

        // FIX: Transfer deposit from proposer to the governance contract to enforce sybil resistance
        token_client.transfer(&proposer, &env.current_contract_address(), &deposit_amount);

        env.storage().persistent().set(&DataKey::Deposit(proposal_id), &deposit_amount);
        env.storage().persistent().set(&DataKey::Proposer(proposal_id), &proposer.clone());
        env.storage().persistent().set(&DataKey::ProposalStatus(proposal_id), &Symbol::new(&env, "Active"));

        env.events().publish((Symbol::new(&env, "ProposalCreated"), proposal_id), (proposer, deposit_amount));
    }

    pub fn cancel_proposal(env: Env, proposer: Address, proposal_id: u64) {
        proposer.require_auth();

        let stored_proposer: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Proposer(proposal_id))
            .unwrap_or_else(|| panic!("Proposal not found"));

        if proposer != stored_proposer {
            panic!("Only the proposer can cancel this proposal");
        }

        let status: Symbol = env.storage().persistent().get(&DataKey::ProposalStatus(proposal_id)).unwrap();
        if status != Symbol::new(&env, "Active") {
            panic!("Proposal is not active");
        }

        let deposit_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Deposit(proposal_id))
            .unwrap_or(0);

        if deposit_amount > 0 {
            let token: Address = env.storage().instance().get(&DataKey::GovernanceToken).unwrap();
            let token_client = TokenClient::new(&env, &token);

            // FIX: Refund the actual token deposit back to the proposer upon cancellation
            token_client.transfer(&env.current_contract_address(), &proposer, &deposit_amount);
        }

        env.storage().persistent().remove(&DataKey::Deposit(proposal_id));
        env.storage().persistent().set(&DataKey::ProposalStatus(proposal_id), &Symbol::new(&env, "Cancelled"));

        env.events().publish((Symbol::new(&env, "ProposalCancelled"), proposal_id), proposer);
    }
}