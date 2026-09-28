#![cfg_attr(not(test), no_std)]
mod types;mod contract;pub mod storage;pub mod scoring;#[cfg(test)]mod test;
use soroban_sdk::{contract,contractimpl,Address,Env};
#[contract]pub struct ReputationRegistry;
#[contractimpl]impl ReputationRegistry{
    pub fn init(env:Env,admin:Address)->Result<(),types::ReputationError>{contract::init(&env,&admin)}
    pub fn record_activity(env:Env,user:Address,activity_type:u32,score_impact:u32)->Result<(),types::ReputationError>{contract::record(&env,&user,activity_type,score_impact)}
    pub fn get_score(env:Env,user:Address)->Result<types::MoiScore,types::ReputationError>{contract::get_score(&env,&user)}
    pub fn get_history(env:Env,user:Address,page:u32,page_size:u32)->soroban_sdk::Vec<types::Activity>{contract::get_history(&env,&user,page,page_size)}
    pub fn pause_registry(env:Env,admin:Address)->Result<(),types::ReputationError>{contract::pause(&env,&admin)}
    pub fn unpause_registry(env:Env,admin:Address)->Result<(),types::ReputationError>{contract::unpause(&env,&admin)}pub fn calc_collateral(env:Env,member:Address)->u32{scoring::calculate_collateral(&env,&member)}pub fn calc_max_size(env:Env,member:Address)->u32{scoring::max_circle_size(&env,&member)}pub fn calc_max_contrib(env:Env,member:Address)->i128{scoring::max_contribution(&env,&member)}

    // ── #53: enhanced on-chain scoring (tier-aware, streak/volume-aware) ──
    pub fn get_moi_score(env:Env,member:Address)->u32{crate::storage::get_score(&env,&member)}
    pub fn get_moi_tier(env:Env,member:Address)->u32{scoring::get_tier(crate::storage::get_score(&env,&member))}

    pub fn record_on_time_payment(env:Env,member:Address,circle_id:Address,amount:i128,round:u32)->Result<u32,types::ReputationError>{
        common::pause::when_not_paused(&env).map_err(|_|types::ReputationError::ContractPaused)?;
        member.require_auth();
        Ok(scoring::record_on_time_payment(&env,&member,&circle_id,amount,round))
    }
    /// Issue #332: the same award as `record_on_time_payment`, scaled by the
    /// contribution's time weight in basis points (0..=10_000). A caller that
    /// passes 10_000 gets the legacy flat award; anything lower is prorated.
    pub fn record_weighted_contribution(env:Env,member:Address,circle_id:Address,amount:i128,round:u32,time_weight_bps:u32)->Result<u32,types::ReputationError>{
        common::pause::when_not_paused(&env).map_err(|_|types::ReputationError::ContractPaused)?;
        member.require_auth();
        Ok(scoring::record_weighted_payment(&env,&member,&circle_id,amount,round,time_weight_bps))
    }
    /// Issue #332: cumulative time-weighted contribution points earned by a member.
    pub fn get_time_weighted_points(env:Env,member:Address)->u64{crate::storage::get_time_weighted_points(&env,&member)}
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
        assert!(true);
    }
}
