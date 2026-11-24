use crate::types::*;
use common::ttl;
use soroban_sdk::{Address, Env, Vec};

/// Get a member's raw MoiScore value
pub fn get_score(env: &Env, member: &Address) -> u32 {
    let key = DataKey::MemberScore(member.clone());
    let score = env.storage().persistent().get(&key).unwrap_or(0u32);
    ttl::extend_persistent(env, &key);
    score
}

/// Set a member's raw MoiScore value
pub fn set_score(env: &Env, member: &Address, score: u32) {
    let key = DataKey::MemberScore(member.clone());
    env.storage().persistent().set(&key, &score);
    ttl::extend_persistent(env, &key);
}

/// Get the current streak count for a member in a circle
pub fn get_streak(env: &Env, member: &Address, circle_id: &Address) -> u32 {
    let key = DataKey::Streak(member.clone(), circle_id.clone());
    let streak = env.storage().persistent().get(&key).unwrap_or(0u32);
    ttl::extend_persistent(env, &key);
    streak
}

/// Increment the streak count
pub fn increment_streak(env: &Env, member: &Address, circle_id: &Address) {
    let current = get_streak(env, member, circle_id);
    let key = DataKey::Streak(member.clone(), circle_id.clone());
    env.storage()
        .persistent()
        .set(&key, &current.saturating_add(1));
    ttl::extend_persistent(env, &key);
}

/// Get number of completed circles
pub fn get_completions(env: &Env, member: &Address) -> u32 {
    let key = DataKey::Completions(member.clone());
    let completions = env.storage().persistent().get(&key).unwrap_or(0u32);
    ttl::extend_persistent(env, &key);
    completions
}

/// Increment completions
pub fn increment_completions(env: &Env, member: &Address) {
    let current = get_completions(env, member);
    let key = DataKey::Completions(member.clone());
    env.storage()
        .persistent()
        .set(&key, &current.saturating_add(1));
    ttl::extend_persistent(env, &key);
}

/// Get number of defaults
pub fn get_defaults(env: &Env, member: &Address) -> u32 {
    let key = DataKey::Defaults(member.clone());
    let defaults = env.storage().persistent().get(&key).unwrap_or(0u32);
    ttl::extend_persistent(env, &key);
    defaults
}

/// Increment defaults
pub fn increment_defaults(env: &Env, member: &Address) {
    let current = get_defaults(env, member);
    let key = DataKey::Defaults(member.clone());
    env.storage()
        .persistent()
        .set(&key, &current.saturating_add(1));
    ttl::extend_persistent(env, &key);
}

/// Get current ledger timestamp in a centralized manner
#[inline]
pub fn get_ledger_timestamp(env: &Env) -> u64 {
    env.ledger().timestamp()
}

/// Add an activity record
pub fn add_activity(env: &Env, member: &Address, activity_type: u32, impact: u32) {
    let key = DataKey::MemberLog(member.clone());
    let mut activities: Vec<Activity> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    activities.push_back(Activity {
        user: member.clone(),
        activity_type,
        score_impact: impact,
        timestamp: get_ledger_timestamp(env),
    });
    env.storage().persistent().set(&key, &activities);
    ttl::extend_persistent(env, &key);
}

pub fn get_last_round(env: &Env, member: &Address, circle_id: &Address) -> u32 {
    let key = DataKey::LastRound(member.clone(), circle_id.clone());
    let round = env.storage().persistent().get(&key).unwrap_or(0u32);
    ttl::extend_persistent(env, &key);
    round
}

pub fn set_last_round(env: &Env, member: &Address, circle_id: &Address, round: u32) {
    let key = DataKey::LastRound(member.clone(), circle_id.clone());
    env.storage().persistent().set(&key, &round);
    ttl::extend_persistent(env, &key);
}
