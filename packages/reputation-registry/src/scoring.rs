use soroban_sdk::{Env, Address};
use crate::storage;
use crate::types::{TIER_BRONZE, TIER_SILVER, TIER_GOLD, TIER_PLATINUM, TIER_DIAMOND, ACTIVITY_CONTRIBUTE, ACTIVITY_COMPLETE, ACTIVITY_DEFAULT};

const SCORE_SILVER: u32 = 201;
const SCORE_GOLD: u32 = 401;
const SCORE_PLATINUM: u32 = 601;
const SCORE_DIAMOND: u32 = 801;

/// Returns the numeric tier constant for a given MoiScore value.
///
/// Compare the result with the `TIER_*` constants from `types`:
/// `TIER_BRONZE`, `TIER_SILVER`, `TIER_GOLD`, `TIER_PLATINUM`, `TIER_DIAMOND`.
pub fn get_tier(score: u32) -> u32 {
    if score >= SCORE_DIAMOND { TIER_DIAMOND }
    else if score >= SCORE_PLATINUM { TIER_PLATINUM }
    else if score >= SCORE_GOLD { TIER_GOLD }
    else if score >= SCORE_SILVER { TIER_SILVER }
    else { TIER_BRONZE }
}

/// Calculate collateral requirement in basis points based on MoiScore tier.
/// Lower tiers = higher collateral. Diamond = 0% collateral.
pub fn calculate_collateral(env: &Env, member: &Address) -> u32 {
    let score = storage::get_score(env, member);
    match get_tier(score) {
        TIER_DIAMOND => 0,    // 0% — fully trusted
        TIER_PLATINUM => 100, // 1%
        TIER_GOLD => 300,     // 3%
        TIER_SILVER => 500,   // 5%
        _ => 1000,             // 10% — default for bronze/unscored
    }
}

/// Returns the maximum circle size a member can create based on their tier.
pub fn max_circle_size(env: &Env, member: &Address) -> u32 {
    let score = storage::get_score(env, member);
    match get_tier(score) {
        TIER_DIAMOND => 100,
        TIER_PLATINUM => 50,
        TIER_GOLD => 20,
        TIER_SILVER => 10,
        _ => 5,
    }
}

/// Returns the maximum contribution amount (in stroops) based on tier.
pub fn max_contribution(env: &Env, member: &Address) -> i128 {
    let score = storage::get_score(env, member);
    match get_tier(score) {
        TIER_DIAMOND => 50_000_0000000,  // 50,000 USDC
        TIER_PLATINUM => 10_000_0000000, // 10,000 USDC
        TIER_GOLD => 2_000_0000000,       // 2,000 USDC
        TIER_SILVER => 500_0000000,       // 500 USDC
        _ => 100_0000000,                  // 100 USDC
    }
}

/// Checks if a member qualifies for a circle with the given minimum MoiScore.
pub fn qualifies_for_circle(env: &Env, member: &Address, min_score: u32, require_completions: bool) -> bool {
    let score = storage::get_score(env, member);
    if score < min_score { return false }
    if require_completions {
        let completions = storage::get_completions(env, member);
        if completions == 0 { return false }
    }
    true
}

/// Base points awarded for a contribution paid at the very start of the window.
pub const BASE_CONTRIBUTION_POINTS: u32 = 10;

/// Full-scale time weight in basis points (1.0x), matching the circle contract's
/// `TIME_WEIGHT_BPS_MAX`.
pub const TIME_WEIGHT_BPS_MAX: u32 = 10_000;

/// Record an on-time payment at full time weight. Returns the new MoiScore.
///
/// Kept as the compatibility entry point: it is exactly
/// `record_weighted_payment(..., TIME_WEIGHT_BPS_MAX)`.
pub fn record_on_time_payment(env: &Env, member: &Address, circle_id: &Address, amount: i128, round: u32) -> u32 {
    record_weighted_payment(env, member, circle_id, amount, round, TIME_WEIGHT_BPS_MAX)
}

/// Issue #332: record a contribution whose reputation award is scaled by its
/// time weight. Returns the new MoiScore.
///
/// # Formula
///
/// ```text
/// w            = min(time_weight_bps, 10_000)          // clamp to 1.0x
/// time_points  = BASE_CONTRIBUTION_POINTS * w / 10_000 // 10 * w / 10_000
/// streak_bonus = min(streak, 10) * 5
/// volume_bonus = min(amount / 100 USDC, 20)
/// new_score    = min(1000, current + time_points + streak_bonus + volume_bonus)
/// ```
///
/// `time_points` is floored, so a contribution paid at the very start of the
/// window (w = 10_000 bps) earns the full 10 base points — identical to the
/// historical flat award — and one paid at the very end of the window earns
/// almost nothing. Rewarding earliness is the point of the weighting: capital
/// that arrived first is what funded the round's payout, so it is worth more
/// than capital that arrived just before the deadline.
///
/// The awarded points are also accumulated under
/// [`crate::storage::get_time_weighted_points`] and written to the activity log
/// with the weighted impact, so the weighting is auditable per contribution.
pub fn record_weighted_payment(env: &Env, member: &Address, circle_id: &Address, amount: i128, round: u32, time_weight_bps: u32) -> u32 {
    let current = storage::get_score(env, member);
    let mut streak = storage::get_streak(env, member, circle_id);
    let last_round = storage::get_last_round(env, member, circle_id);

    if last_round == 0 || round == last_round + 1 {
        storage::increment_streak(env, member, circle_id);
        streak += 1;
    } else {
        env.storage().persistent().set(&crate::types::DataKey::Streak(member.clone(), circle_id.clone()), &1u32);
        streak = 1;
    }
    storage::set_last_round(env, member, circle_id, round);

    let weight: u64 = if time_weight_bps > TIME_WEIGHT_BPS_MAX {
        u64::from(TIME_WEIGHT_BPS_MAX)
    } else {
        u64::from(time_weight_bps)
    };
    // 10 * 10_000 fits comfortably in u64; the division floors by design so the
    // full-weight case reproduces the legacy flat award exactly.
    let time_points: u32 = ((u64::from(BASE_CONTRIBUTION_POINTS) * weight) / u64::from(TIME_WEIGHT_BPS_MAX)) as u32;
    let streak_bonus: u32 = if streak <= 10 { streak * 5 } else { 50 };
    let volume_bonus_raw = (amount / 100_0000000) as u32; // 1 point per 100 USDC
    let volume_bonus: u32 = if volume_bonus_raw > 20 { 20 } else { volume_bonus_raw };

    let new_score = current.saturating_add(time_points).saturating_add(streak_bonus).saturating_add(volume_bonus);
    let capped = if new_score > 1000 { 1000 } else { new_score };

    // Update score
    storage::set_score(env, member, capped);
    // Track the weighted award and log it against the activity history
    storage::add_time_weighted_points(env, member, u64::from(time_points));
    storage::add_activity(env, member, ACTIVITY_CONTRIBUTE, time_points);

    capped
}

/// Record a circle completion. +100 points. Cap at 1000.
pub fn record_circle_completion(env: &Env, member: &Address) -> u32 {
    let current = storage::get_score(env, member);
    let bonus: u32 = 100;
    let new_score = current.saturating_add(bonus);
    let capped = if new_score > 1000 { 1000 } else { new_score };

    storage::increment_completions(env, member);
    storage::set_score(env, member, capped);
    storage::add_activity(env, member, ACTIVITY_COMPLETE, 5);

    capped
}

/// Record a default (missed payment/circle). -200 points. Floor at 0.
pub fn record_default(env: &Env, member: &Address) -> u32 {
    let current = storage::get_score(env, member);
    let penalty: u32 = 200;
    let new_score = if current > penalty { current - penalty } else { 0 };

    storage::increment_defaults(env, member);
    storage::set_score(env, member, new_score);
    storage::add_activity(env, member, ACTIVITY_DEFAULT, 0);

    new_score
}

/// Apply inactivity decay. -5 points per 30 days of inactivity. Floor at 0.
pub fn apply_inactivity_decay(env: &Env, member: &Address, days_inactive: u64) -> u32 {
    let current = storage::get_score(env, member);
    let months = days_inactive / 30;
    let decay: u32 = ((months as u64).saturating_mul(5)) as u32;
    let new_score = if current > decay { current - decay } else { 0 };

    storage::set_score(env, member, new_score);
    new_score
}

pub fn get_score(env: &Env, member: &Address) -> u32 {
    storage::get_score(env, member)
}
