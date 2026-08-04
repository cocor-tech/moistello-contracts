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
