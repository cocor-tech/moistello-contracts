
use soroban_sdk::{contracttype, Address, String};

/// Canonical CircleConfig shared across the workspace.
///
/// This is the single source of truth for circle creation parameters.
/// `circle` and `circle-factory` re-export this type (see their
/// `types.rs`) instead of defining duplicates. See #320.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircleConfig {
    pub organizer: Address,
    pub token: Address,
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
    /// Minimum number of seconds that must elapse between consecutive
    /// `trigger_payout` calls. Set to 0 to disable the cooldown guard.
    /// Fixes #115.
    pub payout_cooldown_seconds: u64,
}
