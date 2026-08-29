#![cfg_attr(not(test), no_std)]

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MathError {
    Overflow = 1,
    Underflow = 2,
    DivisionByZero = 3,
}
pub fn safe_add(a: i128, b: i128) -> Result<i128, MathError> { a.checked_add(b).ok_or(MathError::Overflow) }
pub fn safe_sub(a: i128, b: i128) -> Result<i128, MathError> { a.checked_sub(b).ok_or(MathError::Underflow) }
pub fn safe_mul(a: i128, b: i128) -> Result<i128, MathError> { a.checked_mul(b).ok_or(MathError::Overflow) }
pub fn safe_div(a: i128, b: i128) -> Result<i128, MathError> { if b == 0 { return Err(MathError::DivisionByZero); } a.checked_div(b).ok_or(MathError::Overflow) }
pub fn calculate_percentage(amount: i128, bps: i128) -> Result<i128, MathError> { if bps < 0 || bps > 10_000 { return Err(MathError::Overflow); } safe_div(safe_mul(amount, bps)?, 10_000) }
pub fn apply_fee(amount: i128, fee_bps: i128) -> Result<(i128, i128), MathError> { let fee = calculate_percentage(amount, fee_bps)?; Ok((safe_sub(amount, fee)?, fee)) }
pub fn calculate_penalty(amount: i128, penalty_bps: i128) -> Result<i128, MathError> { calculate_percentage(amount, penalty_bps) }

/// Converts a member's individual shares into a proportional token amount from
/// a pool.
///
/// # Arguments
/// * `member_shares` – the number of shares attributed to the member (must be ≥ 0)
/// * `total_shares`  – the total shares outstanding across all members
/// * `pool_amount`   – the total token amount to be distributed
///
/// # Errors
/// Returns [`MathError::DivisionByZero`] when `total_shares` is zero (pool has
/// no share-holders), preventing a panic inside the contract host.
/// Returns [`MathError::Overflow`] / [`MathError::Underflow`] on arithmetic
/// overflow / underflow in intermediate computations.
pub fn convert_shares(
    member_shares: i128,
    total_shares: i128,
    pool_amount: i128,
) -> Result<i128, MathError> {
    // Guard: total_shares == 0 would cause a division-by-zero panic on-chain.
    // This can happen when a vault or pool loses all members due to a rounding
    // edge case.  Returning a typed error lets the caller handle this gracefully
    // instead of trapping the entire host execution.
    if total_shares == 0 {
        return Err(MathError::DivisionByZero);
    }
    // member_payout = (member_shares * pool_amount) / total_shares
    // Multiplication is performed first to preserve precision; overflow is
    // checked explicitly via safe_mul.
    let numerator = safe_mul(member_shares, pool_amount)?;
    safe_div(numerator, total_shares)
}
