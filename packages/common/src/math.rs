#![cfg_attr(not(test), no_std)]

use soroban_sdk::contracterror;

/// Typed arithmetic errors — never panic, always propagate.
#[contracterror]
#[derive(Debug)]
pub enum MathError {
    Overflow = 1,
    Underflow = 2,
    DivisionByZero = 3,
}

// ---------------------------------------------------------------------------
// i128 safe arithmetic
// ---------------------------------------------------------------------------

pub fn safe_add(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_add(b).ok_or(MathError::Overflow)
}

pub fn safe_sub(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_sub(b).ok_or(MathError::Underflow)
}

pub fn safe_mul(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_mul(b).ok_or(MathError::Overflow)
}

/// Safe signed division.  Explicitly guards against zero denominator so the
/// contract never panics regardless of caller-supplied inputs.
pub fn safe_div(a: i128, b: i128) -> Result<i128, MathError> {
    if b == 0 {
        return Err(MathError::DivisionByZero);
    }
    a.checked_div(b).ok_or(MathError::Overflow)
}

// ---------------------------------------------------------------------------
// u128 safe arithmetic  (used for weighted-share calculations in payout)
// ---------------------------------------------------------------------------

pub fn safe_add_u128(a: u128, b: u128) -> Result<u128, MathError> {
    a.checked_add(b).ok_or(MathError::Overflow)
}

pub fn safe_mul_u128(a: u128, b: u128) -> Result<u128, MathError> {
    a.checked_mul(b).ok_or(MathError::Overflow)
}

/// Safe unsigned division.  Returns `DivisionByZero` when `b == 0` rather
/// than panicking — critical for share-conversion paths where `total_shares`
/// could theoretically be zero due to rounding edge cases.
pub fn safe_div_u128(a: u128, b: u128) -> Result<u128, MathError> {
    if b == 0 {
        return Err(MathError::DivisionByZero);
    }
    Ok(a / b)
}

// ---------------------------------------------------------------------------
// Share / weight conversion helpers
// ---------------------------------------------------------------------------

/// Compute the pro-rata `amount` for a member given their weight out of the
/// total pool.
///
/// Guards:
///  - `total_shares == 0`  →  `DivisionByZero`
///  - intermediate overflow in `net_pool * member_shares`  →  `Overflow`
///
/// This is the canonical path for all weighted-payout arithmetic so that no
/// caller ever divides by zero in ad-hoc code.
pub fn shares_to_amount(net_pool: u128, member_shares: u128, total_shares: u128) -> Result<u128, MathError> {
    if total_shares == 0 {
        return Err(MathError::DivisionByZero);
    }
    let numerator = safe_mul_u128(net_pool, member_shares)?;
    Ok(numerator / total_shares)
}

// ---------------------------------------------------------------------------
// Percentage / fee helpers
// ---------------------------------------------------------------------------

pub fn calculate_percentage(amount: i128, bps: i128) -> Result<i128, MathError> {
    if bps < 0 || bps > 10_000 {
        return Err(MathError::Overflow);
    }
    // safe_div already guards against bps == 0 (10_000 denominator is constant
    // and non-zero, but we use the safe path for correctness).
    safe_div(safe_mul(amount, bps)?, 10_000)
}

pub fn apply_fee(amount: i128, fee_bps: i128) -> Result<(i128, i128), MathError> {
    let fee = calculate_percentage(amount, fee_bps)?;
    Ok((safe_sub(amount, fee)?, fee))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- safe_div -----------------------------------------------------------

    #[test]
    fn test_safe_div_happy_path() {
        assert_eq!(safe_div(100, 4), Ok(25));
        assert_eq!(safe_div(-100, 4), Ok(-25));
        assert_eq!(safe_div(0, 5), Ok(0));
    }

    #[test]
    fn test_safe_div_by_zero_returns_error() {
        assert_eq!(safe_div(100, 0), Err(MathError::DivisionByZero));
        assert_eq!(safe_div(0, 0), Err(MathError::DivisionByZero));
        assert_eq!(safe_div(-1, 0), Err(MathError::DivisionByZero));
    }

    #[test]
    fn test_safe_div_overflow_boundary() {
        // i128::MIN / -1 overflows in two's complement
        assert_eq!(safe_div(i128::MIN, -1), Err(MathError::Overflow));
    }

    // --- safe_div_u128 ------------------------------------------------------

    #[test]
    fn test_safe_div_u128_happy_path() {
        assert_eq!(safe_div_u128(100, 4), Ok(25));
        assert_eq!(safe_div_u128(0, 5), Ok(0));
        assert_eq!(safe_div_u128(u128::MAX, 1), Ok(u128::MAX));
    }

    #[test]
    fn test_safe_div_u128_by_zero_returns_error() {
        assert_eq!(safe_div_u128(1, 0), Err(MathError::DivisionByZero));
        assert_eq!(safe_div_u128(0, 0), Err(MathError::DivisionByZero));
        assert_eq!(safe_div_u128(u128::MAX, 0), Err(MathError::DivisionByZero));
    }

    // --- shares_to_amount ---------------------------------------------------

    #[test]
    fn test_shares_to_amount_happy_path() {
        // 3 members, equal weight → each gets 1/3
        assert_eq!(shares_to_amount(300, 100, 300), Ok(100));
        // 2 members, 3:1 weighting
        assert_eq!(shares_to_amount(400, 300, 400), Ok(300));
        assert_eq!(shares_to_amount(400, 100, 400), Ok(100));
    }

    #[test]
    fn test_shares_to_amount_zero_total_shares_is_error() {
        // This is the guard for the rounding-edge-case described in the issue.
        assert_eq!(
            shares_to_amount(1_000_000, 50, 0),
            Err(MathError::DivisionByZero)
        );
    }

    #[test]
    fn test_shares_to_amount_zero_member_shares() {
        // Member with zero weight receives nothing (not an error).
        assert_eq!(shares_to_amount(1_000_000, 0, 1_000_000), Ok(0));
    }

    #[test]
    fn test_shares_to_amount_overflow_on_numerator() {
        // net_pool × member_shares overflows u128
        assert_eq!(
            shares_to_amount(u128::MAX, u128::MAX, u128::MAX),
            Err(MathError::Overflow)
        );
    }

    #[test]
    fn test_shares_to_amount_single_member_full_pool() {
        // One member with all shares → receives full pool
        assert_eq!(shares_to_amount(500_000_000, 1, 1), Ok(500_000_000));
    }

    // --- safe_add / safe_sub / safe_mul -------------------------------------

    #[test]
    fn test_safe_add_overflow() {
        assert_eq!(safe_add(i128::MAX, 1), Err(MathError::Overflow));
    }

    #[test]
    fn test_safe_sub_underflow() {
        assert_eq!(safe_sub(i128::MIN, 1), Err(MathError::Underflow));
    }

    #[test]
    fn test_safe_mul_overflow() {
        assert_eq!(safe_mul(i128::MAX, 2), Err(MathError::Overflow));
    }

    // --- safe_div_u128 edge cases -------------------------------------------

    #[test]
    fn test_safe_div_u128_truncates_remainder() {
        // u128 div is truncating (floor for unsigned)
        assert_eq!(safe_div_u128(7, 2), Ok(3));
    }

    // --- calculate_percentage -----------------------------------------------

    #[test]
    fn test_calculate_percentage_happy_path() {
        assert_eq!(calculate_percentage(1_000, 500), Ok(50));   // 5%
        assert_eq!(calculate_percentage(1_000, 0), Ok(0));      // 0%
        assert_eq!(calculate_percentage(1_000, 10_000), Ok(1_000)); // 100%
    }

    #[test]
    fn test_calculate_percentage_invalid_bps() {
        assert_eq!(calculate_percentage(1_000, -1), Err(MathError::Overflow));
        assert_eq!(calculate_percentage(1_000, 10_001), Err(MathError::Overflow));
    }

    // --- apply_fee ----------------------------------------------------------

    #[test]
    fn test_apply_fee_happy_path() {
        let (net, fee) = apply_fee(1_000, 500).unwrap(); // 5% fee
        assert_eq!(fee, 50);
        assert_eq!(net, 950);
    }

    #[test]
    fn test_apply_fee_zero_fee() {
        let (net, fee) = apply_fee(1_000, 0).unwrap();
        assert_eq!(fee, 0);
        assert_eq!(net, 1_000);
    }
}
