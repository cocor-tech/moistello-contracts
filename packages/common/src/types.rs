#![cfg_attr(not(test), no_std)]
use soroban_sdk::{contracttype, Address, String};

#[contracttype]
#[derive(Clone, Debug)]
pub struct CircleConfig {
    pub organizer: Address,
    pub name: String,
    pub contribution_amount: i128,
    pub max_members: u32,
    pub payout_type: u32,
    pub total_rounds: u32,
    pub contribution_deadline_seconds: u64,
    pub min_moi_score: u32,
    pub collateral_amount: i128,
    pub penalty_bps: u32,
    pub grace_period_seconds: u64,
    pub max_strikes: u32,
    pub slug: String,
    pub fee_bps: u32,
}
