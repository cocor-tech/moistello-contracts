//! Governance contract — all public handler functions follow the **uniform
//! Soroban-native `&Env`-based API pattern**: every function takes `env: &Env`
//! as its first parameter and returns `Result<_, GovernanceError>` (or a plain
//! value for pure reads).  There are no `ExecCtx`, `QueryCtx`, or
//! `MessageInfo` parameters; those belong to CosmWasm and must never appear
//! here.  All mutation functions perform access-control checks first, before
//! touching storage.
use soroban_sdk::{Address, BytesN, Env, Val, Vec};
use crate::types::*;
use common::{math, pause};

const BPS_DENOM: i128 = 10_000;

/// Fixed delay a queued `GovernanceConfig` update must wait before it can be
/// executed. Deliberately a hardcoded constant rather than sourced from
/// `GovernanceConfig` itself: if it were configurable, an admin could queue
/// a config change that zeroes it out and instantly self-approve any future
/// change, defeating the whole point of a parameter-change timelock.
const CONFIG_TIMELOCK_SECONDS: u64 = 172_800; // 48 hours

fn validate_config(config: &GovernanceConfig) -> Result<(), GovernanceError> {
    if config.min_proposal_deposit < 0 || config.proposal_deposit < config.min_proposal_deposit {
        return Err(GovernanceError::InvalidConfig);
    }
    if config.pass_threshold_bps == 0 || config.pass_threshold_bps > BPS_DENOM as u32 {
        return Err(GovernanceError::InvalidConfig);
    }
    Ok(())
}

fn add_proposal_to_status_index(env: &Env, status: &ProposalStatus, proposal_id: u64) {
    let mut list: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::ProposalsByStatus(status.clone()))
        .unwrap_or_else(|| Vec::new(env));
    list.push_back(proposal_id);
    env.storage()
        .persistent()
        .set(&DataKey::ProposalsByStatus(status.clone()), &list);
}

fn remove_proposal_from_status_index(env: &Env, status: &ProposalStatus, proposal_id: u64) {
    let list: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::ProposalsByStatus(status.clone()))
        .unwrap_or_else(|| Vec::new(env));
    let mut new_list = Vec::new(env);
    for i in 0..list.len() {
        if let Some(id) = list.get(i) {
            if id != proposal_id {
                new_list.push_back(id);
            }
        }
    }
    env.storage()
        .persistent()
        .set(&DataKey::ProposalsByStatus(status.clone()), &new_list);
}

pub fn init(env: &Env, admin: &Address, config: &GovernanceConfig) -> Result<(), GovernanceError> {
    admin.require_auth();
    if env.storage().instance().has(&DataKey::Admin) {
        return Err(GovernanceError::AlreadyInitialized);
    }
    validate_config(config)?;
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::Config, config);
    env.storage().instance().set(&DataKey::ProposalCount, &0u64);
    Ok(())
}

/// Creates a proposal and stakes `deposit_amount` (bookkept as an i128
/// balance on this contract, matching the rest of this workspace's
/// bookkeeping-only pattern — no real token contract is integrated here).
/// Proposals go straight to `Active` (voting begins immediately): no
/// separate draft-editing period was specified in the source issue, so
/// `Draft` is included in `ProposalStatus` for lifecycle fidelity but is
/// not currently a reachable state.
pub fn create_proposal(
    env: &Env,
    proposer: &Address,
    deposit_amount: i128,
    action: ProposalAction,
    description: BytesN<32>,
) -> Result<u64, GovernanceError> {
    pause::when_not_paused(env).map_err(|_| GovernanceError::ContractPaused)?;
    proposer.require_auth();
    let config: GovernanceConfig = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(GovernanceError::NotInitialized)?;
    if deposit_amount < config.min_proposal_deposit {
        return Err(GovernanceError::InsufficientDeposit);
    }
    let id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::ProposalCount)
        .unwrap_or(0);
    let now = env.ledger().timestamp();
    let voting_ends_at = now
        .checked_add(config.voting_period_seconds)
        .ok_or(GovernanceError::InvalidConfig)?;
    let proposal = Proposal {
        id,
        proposer: proposer.clone(),
        deposit_amount,
        action,
        description,
        status: ProposalStatus::Active,
        created_at: now,
        voting_ends_at,
        timelock_ends_at: 0,
        votes_for: 0,
        votes_against: 0,
        votes_abstain: 0,
    };
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(id), &proposal);
    env.storage()
        .persistent()
        .set(&DataKey::Deposit(id), &deposit_amount);
    env.storage().instance().set(
        &DataKey::ProposalCount,
        &id.checked_add(1).ok_or(GovernanceError::InvalidConfig)?,
    );
    add_proposal_to_status_index(env, &ProposalStatus::Active, id);
    ProposalCreated {
        id,
        proposer: proposer.clone(),
        deposit_amount,
        voting_ends_at,
    }
    .publish(env);
    Ok(id)
}

/// Vote power is flat one-address-one-vote (no governance-token balance
/// exists anywhere in this workspace to weight by; see the `GovernanceConfig`
/// doc comment in types.rs for the same reasoning behind `quorum_votes`).
pub fn cast_vote(
    env: &Env,
    voter: &Address,
    proposal_id: u64,
    vote: VoteType,
) -> Result<(), GovernanceError> {
    pause::when_not_paused(env).map_err(|_| GovernanceError::ContractPaused)?;
    voter.require_auth();
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;
    if proposal.status != ProposalStatus::Active {
        return Err(GovernanceError::VotingNotActive);
    }
    let now = env.ledger().timestamp();
    if now > proposal.voting_ends_at {
        return Err(GovernanceError::VotingEnded);
    }
    if env
        .storage()
        .persistent()
        .has(&DataKey::Vote(proposal_id, voter.clone()))
    {
        return Err(GovernanceError::AlreadyVoted);
    }
    let vote_power: i128 = 1;
    match vote {
        VoteType::For => {
            proposal.votes_for = proposal
                .votes_for
                .checked_add(vote_power)
                .ok_or(GovernanceError::InvalidConfig)?;
        }
        VoteType::Against => {
            proposal.votes_against = proposal
                .votes_against
                .checked_add(vote_power)
                .ok_or(GovernanceError::InvalidConfig)?;
        }
        VoteType::Abstain => {
            proposal.votes_abstain = proposal
                .votes_abstain
                .checked_add(vote_power)
                .ok_or(GovernanceError::InvalidConfig)?;
        }
    }
    env.storage().persistent().set(
        &DataKey::Vote(proposal_id, voter.clone()),
        &VoteRecord {
            voter: voter.clone(),
            vote: vote.clone(),
            vote_power,
            timestamp: now,
        },
    );
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal_id), &proposal);
    VoteCast {
        id: proposal_id,
        voter: voter.clone(),
        vote,
        vote_power,
    }
    .publish(env);
    Ok(())
}

/// Finalize a proposal after its voting period has ended.
///
/// Design note (#244): This function is intentionally permissionless (no caller
/// authentication check required), following the Compound Governor / OpenZeppelin
/// Governor standard. Anyone or any automated keeper can trigger finalization
/// once `voting_ends_at` has passed.
///
/// Security & Timelock Guarantees:
/// To prevent fast-tracking execution, when a proposal succeeds, its execution
/// timelock (`timelock_ends_at`) is anchored to the finalization timestamp:
/// `now + config.timelock_seconds`. Thus, even if finalized immediately after voting
/// ends, execution is strictly locked until the full timelock delay has elapsed,
/// ensuring sufficient review time and preventing surprise executions.
pub fn finalize_proposal(env: &Env, proposal_id: u64) -> Result<(), GovernanceError> {
    let config: GovernanceConfig = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(GovernanceError::NotInitialized)?;
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;
    if proposal.status != ProposalStatus::Active {
        return Err(GovernanceError::VotingNotActive);
    }
    let now = env.ledger().timestamp();
    if now <= proposal.voting_ends_at {
        return Err(GovernanceError::VotingNotActive);
    }
    let total_votes = proposal.votes_for + proposal.votes_against + proposal.votes_abstain;
    let quorum_met = total_votes >= config.quorum_votes as i128;
    let decisive = proposal.votes_for + proposal.votes_against;
    // Use safe_div to guard against a zero decisive denominator — a raw `/`
    // would panic on-chain if both votes_for and votes_against are zero.
    // The `decisive > 0` short-circuit prevents the division in practice, but
    // relying on evaluation order for safety is fragile; safe_div makes the
    // invariant explicit and returns a typed MathError if ever reached.
    let votes_for_scaled = proposal
        .votes_for
        .checked_mul(BPS_DENOM)
        .ok_or(GovernanceError::InvalidConfig)?;
    let passed = quorum_met
        && decisive > 0
        && math::safe_div(votes_for_scaled, decisive)
            .map_err(|_| GovernanceError::InvalidConfig)?
            >= config.pass_threshold_bps as i128;
    remove_proposal_from_status_index(env, &ProposalStatus::Active, proposal_id);
    if passed {
        proposal.timelock_ends_at = now
            .checked_add(config.timelock_seconds)
            .ok_or(GovernanceError::InvalidConfig)?;
        proposal.status = ProposalStatus::Queued;
        add_proposal_to_status_index(env, &ProposalStatus::Queued, proposal_id);
        ProposalStatusChanged {
            id: proposal_id,
            status: ProposalStatus::Queued,
        }
        .publish(env);
    } else {
        proposal.status = ProposalStatus::Defeated;
        add_proposal_to_status_index(env, &ProposalStatus::Defeated, proposal_id);
        ProposalStatusChanged {
            id: proposal_id,
            status: ProposalStatus::Defeated,
        }
        .publish(env);
    }
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal_id), &proposal);
    Ok(())
}

/// Permissionless execution after the timelock has elapsed.
pub fn execute_proposal(env: &Env, proposal_id: u64) -> Result<(), GovernanceError> {
    pause::when_not_paused(env).map_err(|_| GovernanceError::ContractPaused)?;
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;
    if proposal.status != ProposalStatus::Queued {
        return Err(GovernanceError::ProposalNotSucceeded);
    }
    let now = env.ledger().timestamp();
    if now < proposal.timelock_ends_at {
        return Err(GovernanceError::TimelockNotElapsed);
    }
    let args: Vec<Val> = proposal.action.args.clone();
    env.invoke_contract::<Val>(
        &proposal.action.target_contract,
        &proposal.action.method,
        args,
    );
    proposal.status = ProposalStatus::Executed;
    remove_proposal_from_status_index(env, &ProposalStatus::Queued, proposal_id);
    add_proposal_to_status_index(env, &ProposalStatus::Executed, proposal_id);
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal_id), &proposal);
    ProposalExecuted {
        id: proposal_id,
        executed_by: env.current_contract_address(),
    }
    .publish(env);
    Ok(())
}

/// Cancel own proposal — only while Active and before any votes have been
/// cast (adapted from "before voting starts": voting begins immediately at
/// creation in this implementation, see `create_proposal`). Refunds the
/// bookkept deposit.
pub fn cancel_proposal(
    env: &Env,
    caller: &Address,
    proposal_id: u64,
) -> Result<(), GovernanceError> {
    caller.require_auth();
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;
    if &proposal.proposer != caller {
        return Err(GovernanceError::NotProposer);
    }
    if proposal.status != ProposalStatus::Active {
        return Err(GovernanceError::ProposalNotDraftOrActive);
    }
    if proposal.votes_for + proposal.votes_against + proposal.votes_abstain > 0 {
        return Err(GovernanceError::VotingAlreadyStarted);
    }
    proposal.status = ProposalStatus::Cancelled;
    remove_proposal_from_status_index(env, &ProposalStatus::Active, proposal_id);
    add_proposal_to_status_index(env, &ProposalStatus::Cancelled, proposal_id);
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal_id), &proposal);
    env.storage()
        .persistent()
        .remove(&DataKey::Deposit(proposal_id));
    ProposalCancelled {
        id: proposal_id,
        cancelled_by: caller.clone(),
    }
    .publish(env);
    Ok(())
}

/// Queues a `GovernanceConfig` change, executable no sooner than
/// `CONFIG_TIMELOCK_SECONDS` (48h) from now. Only one update may be queued
/// at a time — cancel the pending one first to replace it — so the
/// executable timestamp a caller observes can't be silently pushed back out
/// by re-queuing.
pub fn queue_config_update(
    env: &Env,
    admin: &Address,
    new_config: GovernanceConfig,
) -> Result<(), GovernanceError> {
    admin.require_auth();
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;
    if admin != &s {
        return Err(GovernanceError::Unauthorized);
    }
    if env.storage().instance().has(&DataKey::PendingConfig) {
        return Err(GovernanceError::ConfigUpdateAlreadyQueued);
    }
    validate_config(&new_config)?;
    let now = env.ledger().timestamp();
    let executable_at = now
        .checked_add(CONFIG_TIMELOCK_SECONDS)
        .ok_or(GovernanceError::InvalidConfig)?;
    env.storage().instance().set(
        &DataKey::PendingConfig,
        &PendingConfigUpdate {
            new_config,
            queued_at: now,
            executable_at,
        },
    );
    ConfigUpdateQueued {
        queued_by: admin.clone(),
        executable_at,
    }
    .publish(env);
    Ok(())
}

/// Permissionless, like `execute_proposal`: the timelock is the control,
/// not who happens to submit the transaction once it has elapsed.
pub fn execute_config_update(env: &Env) -> Result<(), GovernanceError> {
    let pending: PendingConfigUpdate = env
        .storage()
        .instance()
        .get(&DataKey::PendingConfig)
        .ok_or(GovernanceError::NoPendingConfigUpdate)?;
    let now = env.ledger().timestamp();
    if now < pending.executable_at {
        return Err(GovernanceError::TimelockNotElapsed);
    }
    env.storage()
        .instance()
        .set(&DataKey::Config, &pending.new_config);
    env.storage().instance().remove(&DataKey::PendingConfig);
    ConfigUpdated {
        updated_by: env.current_contract_address(),
    }
    .publish(env);
    Ok(())
}

/// Admin-only: cancel a queued config update at any point during (or after)
/// the timelock, before it has been executed.
pub fn cancel_config_update(env: &Env, admin: &Address) -> Result<(), GovernanceError> {
    admin.require_auth();
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;
    if admin != &s {
        return Err(GovernanceError::Unauthorized);
    }
    if !env.storage().instance().has(&DataKey::PendingConfig) {
        return Err(GovernanceError::NoPendingConfigUpdate);
    }
    env.storage().instance().remove(&DataKey::PendingConfig);
    ConfigUpdateCancelled {
        cancelled_by: admin.clone(),
    }
    .publish(env);
    Ok(())
}

pub fn get_pending_config_update(env: &Env) -> Option<PendingConfigUpdate> {
    env.storage().instance().get(&DataKey::PendingConfig)
}

pub fn get_proposal(env: &Env, id: u64) -> Result<Proposal, GovernanceError> {
    env.storage()
        .persistent()
        .get(&DataKey::Proposal(id))
        .ok_or(GovernanceError::ProposalNotFound)
}

/// Efficient status-indexed lookup (fixes #243)
pub fn get_proposals(env: &Env, status: ProposalStatus, limit: u32) -> Vec<Proposal> {
    let ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::ProposalsByStatus(status))
        .unwrap_or_else(|| Vec::new(env));
    let mut out = Vec::new(env);
    let mut i = 0u32;
    while i < ids.len() && (out.len() as u32) < limit {
        if let Some(id) = ids.get(i) {
            if let Some(p) = env
                .storage()
                .persistent()
                .get::<DataKey, Proposal>(&DataKey::Proposal(id))
            {
                out.push_back(p);
            }
        }
        i += 1;
    }
    out
}

pub fn get_vote(env: &Env, proposal_id: u64, voter: &Address) -> Option<VoteRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::Vote(proposal_id, voter.clone()))
}

/// Flat vote power (see `cast_vote` doc comment).
pub fn get_vote_power(env: &Env, voter: &Address) -> i128 {
    if let Some(staking_addr) = env.storage().instance().get::<_, Address>(&DataKey::StakingContract) {
        env.invoke_contract(&staking_addr, &soroban_sdk::Symbol::new(env, "get_voting_power"), soroban_sdk::vec![env, voter.to_val()])
    } else {
        1
    }
}

pub fn set_staking_contract(env: &Env, admin: &Address, staking: &Address) -> Result<(), GovernanceError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;
    if admin != &s {
        return Err(GovernanceError::Unauthorized);
    }
    admin.require_auth();
    env.storage().instance().set(&DataKey::StakingContract, staking);
    Ok(())
}

pub fn get_config(env: &Env) -> Result<GovernanceConfig, GovernanceError> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(GovernanceError::NotInitialized)
}

pub fn pause(env: &Env, admin: &Address) -> Result<(), GovernanceError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;
    if admin != &s {
        return Err(GovernanceError::Unauthorized);
    }
    pause::pause(env, admin).map_err(|_| GovernanceError::ContractPaused)
}

pub fn unpause(env: &Env, admin: &Address) -> Result<(), GovernanceError> {
    let s: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(GovernanceError::NotInitialized)?;
    if admin != &s {
        return Err(GovernanceError::Unauthorized);
    }
    pause::unpause(env, admin).map_err(|_| GovernanceError::ContractPaused)
}
