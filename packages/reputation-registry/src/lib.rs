#![cfg_attr(not(test), no_std)]
mod types;mod contract;mod storage;pub mod scoring;#[cfg(test)]mod test;
use soroban_sdk::{contract,contractimpl,Address,Env};
#[contract]pub struct ReputationRegistry;
#[contractimpl]impl ReputationRegistry{
    pub fn init(env:Env,admin:Address){contract::init(&env,&admin)}
    pub fn record_activity(env:Env,user:Address,activity_type:u32,score_impact:u32)->Result<(),types::ReputationError>{contract::record(&env,&user,activity_type,score_impact)}
    pub fn get_score(env:Env,user:Address)->types::MoiScore{contract::get_score(&env,&user)}
    pub fn get_history(env:Env,user:Address)->soroban_sdk::Vec<types::Activity>{contract::get_history(&env,&user)}
    pub fn pause(env:Env,admin:Address)->Result<(),types::ReputationError>{contract::pause(&env,&admin)}
    pub fn unpause(env:Env,admin:Address)->Result<(),types::ReputationError>{contract::unpause(&env,&admin)}pub fn calc_collateral(env:Env,member:Address)->u32{scoring::calculate_collateral(&env,&member)}pub fn calc_max_size(env:Env,member:Address)->u32{scoring::max_circle_size(&env,&member)}pub fn calc_max_contrib(env:Env,member:Address)->i128{scoring::max_contribution(&env,&member)}

    // ── #53: enhanced on-chain scoring (tier-aware, streak/volume-aware) ──
    // These wrap the already-implemented functions in `scoring.rs`, which
    // previously existed as free functions with no public contract entry
    // point — unreachable from outside the contract (e.g. from Circle).
    // `get_moi_score`/`get_moi_tier` are also new: the pre-existing public
    // `get_score` reads a single global (non-per-member) MoiScore record via
    // `DataKey::Scores`, which does not key by user at all. The tier/limit
    // helpers above (`calc_collateral` etc.) instead read the correct,
    // per-member `DataKey::MemberScore(Address)` via `storage::get_score`.
    // These two queries expose that same, correct per-member score/tier so
    // callers (like Circle's join/contribute checks) don't inherit the
    // pre-existing global-score bug.
    pub fn get_moi_score(env:Env,member:Address)->u32{crate::storage::get_score(&env,&member)}
    pub fn get_moi_tier(env:Env,member:Address)->u32{scoring::get_tier(crate::storage::get_score(&env,&member))}

    pub fn record_on_time_payment(env:Env,member:Address,circle_id:Address,amount:i128)->Result<u32,types::ReputationError>{
        common::pause::when_not_paused(&env).map_err(|_|types::ReputationError::ContractPaused)?;
        member.require_auth();
        Ok(scoring::record_on_time_payment(&env,&member,&circle_id,amount))
    }
    pub fn record_circle_completion(env:Env,member:Address)->Result<u32,types::ReputationError>{
        common::pause::when_not_paused(&env).map_err(|_|types::ReputationError::ContractPaused)?;
        member.require_auth();
        Ok(scoring::record_circle_completion(&env,&member))
    }
    pub fn record_default(env:Env,member:Address)->Result<u32,types::ReputationError>{
        common::pause::when_not_paused(&env).map_err(|_|types::ReputationError::ContractPaused)?;
        member.require_auth();
        Ok(scoring::record_default(&env,&member))
    }
    pub fn apply_inactivity_decay(env:Env,member:Address,days_inactive:u64)->Result<u32,types::ReputationError>{
        common::pause::when_not_paused(&env).map_err(|_|types::ReputationError::ContractPaused)?;
        member.require_auth();
        Ok(scoring::apply_inactivity_decay(&env,&member,days_inactive))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_smoke_compile() { assert!(true); }
    #[test]
    fn test_types_compile() {
        // Verify contract types compile correctly
        assert!(true);
    }
}

