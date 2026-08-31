use soroban_sdk::{contracttype,contracterror,Address};

pub const ACTIVITY_JOIN:u32=0;
pub const ACTIVITY_CONTRIBUTE:u32=1;
pub const ACTIVITY_COMPLETE:u32=2;
pub const ACTIVITY_DEFAULT:u32=3;
pub const ACTIVITY_PAYOUT_RECEIVED:u32=4;

pub const TIER_BRONZE:u32=0;
pub const TIER_SILVER:u32=1;
pub const TIER_GOLD:u32=2;
pub const TIER_PLATINUM:u32=3;
pub const TIER_DIAMOND:u32=4;

/// On-chain MoiScore record for a member.
///
/// When the `serde` feature is enabled this type serialises to camelCase JSON
/// so that off-chain clients (Go backend, frontend) receive field names that
/// are consistent with the rest of the Moistello ecosystem.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct MoiScore {
    pub user: Address,
    pub score: u32,
    pub tier: u32,
    pub total_circles: u32,
    pub completed_circles: u32,
    pub defaulted_circles: u32,
    pub streak_count: u32,
    pub last_activity_at: u64,
    pub updated_at: u64,
}

/// Individual activity record appended to a member's history.
///
/// Serialises to camelCase JSON when the `serde` feature is enabled, matching
/// the camelCase convention used by the circle contract and the Go backend.
#[contracttype]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Activity {
    pub user: Address,
    pub activity_type: u32,
    pub score_impact: u32,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Score(Address),
    UserActivityPage(Address, u32),
    UserActivityCount(Address),
    Streak(Address, Address),
    Completions(Address),
    MemberScore(Address),
    Defaults(Address),
    MemberLog(Address),
}

#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReputationError {
    NotInitialized = 1,
    Unauthorized = 2,
    ContractPaused = 3,
    InvalidActivityType = 4,
    ScoreNotFound = 5,
    InvalidScoreImpact = 6,
    Overflow = 7,
}

/// Event emitted when a member's activity is recorded.
///
/// Serialises to camelCase JSON when the `serde` feature is enabled.
#[contracttype]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ActivityRecorded {
    pub user: Address,
    pub activity_type: u32,
    pub score_impact: u32,
    pub new_score: u32,
}

/// Event emitted when a member's score tier changes.
///
/// Serialises to camelCase JSON when the `serde` feature is enabled.
#[contracttype]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ScoreUpdated {
    pub user: Address,
    pub old_score: u32,
    pub new_score: u32,
    pub tier: u32,
}
