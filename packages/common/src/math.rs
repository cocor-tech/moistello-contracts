use soroban_sdk::contracterror;
#[contracterror] #[derive(Debug)] pub enum MathError { Overflow=1, Underflow=2, DivisionByZero=3 }
pub fn safe_add(a: i128, b: i128) -> Result<i128, MathError> { a.checked_add(b).ok_or(MathError::Overflow) }
pub fn safe_sub(a: i128, b: i128) -> Result<i128, MathError> { a.checked_sub(b).ok_or(MathError::Underflow) }
pub fn safe_mul(a: i128, b: i128) -> Result<i128, MathError> { a.checked_mul(b).ok_or(MathError::Overflow) }
pub fn safe_div(a: i128, b: i128) -> Result<i128, MathError> { if b == 0 { return Err(MathError::DivisionByZero); } a.checked_div(b).ok_or(MathError::Overflow) }
pub fn calculate_percentage(amount: i128, bps: i128) -> Result<i128, MathError> { if bps < 0 || bps > 10_000 { return Err(MathError::Overflow); } safe_div(safe_mul(amount, bps)?, 10_000) }
pub fn apply_fee(amount: i128, fee_bps: i128) -> Result<(i128, i128), MathError> { let fee = calculate_percentage(amount, fee_bps)?; Ok((safe_sub(amount, fee)?, fee)) }

/// Convert a number of shares into an asset amount using the pool's exchange
/// rate.
///
/// # Formula
///   assets = shares * total_assets / total_shares
///
/// # Zero-division guard
/// Both `total_shares` and `total_assets` must be non-zero before this is
/// called.  A zero `total_shares` (e.g. due to a rounding edge case in a
/// vault or pool) would otherwise cause a panic or silent data corruption.
/// We return `MathError::DivisionByZero` rather than panicking so the caller
/// can handle the degenerate case gracefully (e.g. return 0 assets or
/// reinitialise the pool).
pub fn shares_to_assets(shares: i128, total_assets: i128, total_shares: i128) -> Result<i128, MathError> {
    if total_shares == 0 {
        return Err(MathError::DivisionByZero);
    }
    // shares * total_assets / total_shares  —  multiply first to preserve precision.
    let numerator = safe_mul(shares, total_assets)?;
    safe_div(numerator, total_shares)
}

/// Convert an asset amount into shares using the pool's exchange rate.
///
/// # Formula
///   shares = assets * total_shares / total_assets
///
/// # Zero-division guard
/// `total_assets` must be non-zero; returns `MathError::DivisionByZero`
/// otherwise.  Callers are responsible for seeding the pool with a non-zero
/// asset balance before allowing deposits.
pub fn assets_to_shares(assets: i128, total_shares: i128, total_assets: i128) -> Result<i128, MathError> {
    if total_assets == 0 {
        return Err(MathError::DivisionByZero);
    }
    let numerator = safe_mul(assets, total_shares)?;
    safe_div(numerator, total_assets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Property: for every valid (amount, fee_bps) the fee and net must sum
    // back to the original amount — no tokens must be created or destroyed.
    // We allow a rounding tolerance of 1 stropp due to integer truncation in
    // the fee calculation (fee = amount * bps / 10_000 uses floor division).
    proptest! {
        #[test]
        fn prop_apply_fee_fee_plus_net_equals_amount(
            // Restrict amount to [0, i128::MAX/2] as specified in the issue.
            amount in 0i128..=(i128::MAX / 2),
            // fee_bps is a basis-points value in [0, 10_000].
            fee_bps in 0i128..=10_000i128,
        ) {
            let result = apply_fee(amount, fee_bps);
            // apply_fee must never panic — always return a Result.
            prop_assume!(result.is_ok());
            let (net, fee) = result.unwrap();

            // Invariant: fee + net == amount (within rounding tolerance of 1).
            let sum = fee + net;
            let diff = (sum - amount).abs();
            prop_assert!(
                diff <= 1,
                "fee({}) + net({}) = {} != amount({}); diff = {}",
                fee, net, sum, amount, diff
            );

            // Sanity: neither component is negative.
            prop_assert!(fee >= 0, "fee must be non-negative, got {}", fee);
            prop_assert!(net >= 0, "net must be non-negative, got {}", net);
        }

        // Edge: zero amount must always yield (0, 0).
        #[test]
        fn prop_apply_fee_zero_amount(fee_bps in 0i128..=10_000i128) {
            let (net, fee) = apply_fee(0, fee_bps).expect("apply_fee(0, _) must not error");
            prop_assert_eq!(net, 0);
            prop_assert_eq!(fee, 0);
        }

        // Edge: zero fee_bps means the full amount is returned as net.
        #[test]
        fn prop_apply_fee_zero_bps(amount in 0i128..=(i128::MAX / 2)) {
            let (net, fee) = apply_fee(amount, 0).expect("apply_fee(_, 0) must not error");
            prop_assert_eq!(fee, 0);
            prop_assert_eq!(net, amount);
        }

        // Edge: 10_000 bps (100%) means the full amount is charged as fee.
        #[test]
        fn prop_apply_fee_full_bps(amount in 0i128..=(i128::MAX / 2)) {
            let (net, fee) = apply_fee(amount, 10_000).expect("apply_fee(_, 10_000) must not error");
            prop_assert_eq!(fee, amount);
            prop_assert_eq!(net, 0);
        }
    }

    // -------------------------------------------------------------------------
    // Fix 4 — zero-division guards for share conversion math
    // -------------------------------------------------------------------------

    #[test]
    fn test_shares_to_assets_zero_total_shares_returns_error() {
        let err = shares_to_assets(100, 1_000, 0).unwrap_err();
        assert!(matches!(err, MathError::DivisionByZero));
    }

    #[test]
    fn test_assets_to_shares_zero_total_assets_returns_error() {
        let err = assets_to_shares(100, 1_000, 0).unwrap_err();
        assert!(matches!(err, MathError::DivisionByZero));
    }

    #[test]
    fn test_shares_to_assets_basic() {
        // 50 shares out of 100 total, over 200 assets => 100 assets
        let assets = shares_to_assets(50, 200, 100).unwrap();
        assert_eq!(assets, 100);
    }

    #[test]
    fn test_assets_to_shares_basic() {
        // 100 assets into a pool of 200 assets / 100 shares => 50 shares
        let shares = assets_to_shares(100, 100, 200).unwrap();
        assert_eq!(shares, 50);
    }

    #[test]
    fn test_shares_to_assets_zero_shares_gives_zero() {
        let assets = shares_to_assets(0, 1_000, 500).unwrap();
        assert_eq!(assets, 0);
    }

    #[test]
    fn test_assets_to_shares_zero_assets_gives_zero() {
        let shares = assets_to_shares(0, 500, 1_000).unwrap();
        assert_eq!(shares, 0);
    }

    // Overflow protection: numerator must not overflow i128.
    #[test]
    fn test_shares_to_assets_overflow_returns_error() {
        let err = shares_to_assets(i128::MAX, i128::MAX, 1).unwrap_err();
        assert!(matches!(err, MathError::Overflow));
    }
}
