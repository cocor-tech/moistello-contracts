use soroban_sdk::contracterror;

#[contracterror]
#[derive(Debug)]
pub enum MathError {
    Overflow = 1,
    Underflow = 2,
    DivisionByZero = 3,
}

pub fn safe_add(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_add(b).ok_or(MathError::Overflow)
}

pub fn safe_sub(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_sub(b).ok_or(MathError::Underflow)
}

pub fn safe_mul(a: i128, b: i128) -> Result<i128, MathError> {
    a.checked_mul(b).ok_or(MathError::Overflow)
}

pub fn safe_div(a: i128, b: i128) -> Result<i128, MathError> {
    if b == 0 {
        return Err(MathError::DivisionByZero);
    }
    a.checked_div(b).ok_or(MathError::Overflow)
}

pub fn calculate_percentage(amount: i128, bps: i128) -> Result<i128, MathError> {
    if bps < 0 || bps > 10_000 {
        return Err(MathError::Overflow);
    }
    safe_div(safe_mul(amount, bps)?, 10_000)
}

pub fn apply_fee(amount: i128, fee_bps: i128) -> Result<(i128, i128), MathError> {
    let fee = calculate_percentage(amount, fee_bps)?;
    Ok((safe_sub(amount, fee)?, fee))
}

/// Convert a number of shares into the equivalent asset amount.
///
/// Formula: `assets = (shares * total_assets) / total_shares`
///
/// Returns `Err(MathError::DivisionByZero)` if `total_shares == 0`, which
/// would otherwise cause an on-chain panic (contract abort). Callers must
/// treat this as "the pool is empty — no conversion possible" and surface
/// it via a typed contract error rather than propagating a panic.
pub fn shares_to_assets(
    shares: i128,
    total_shares: i128,
    total_assets: i128,
) -> Result<i128, MathError> {
    if total_shares == 0 {
        return Err(MathError::DivisionByZero);
    }
    safe_div(safe_mul(shares, total_assets)?, total_shares)
}

/// Convert an asset amount into the equivalent number of shares.
///
/// Formula: `shares = (assets * total_shares) / total_assets`
///
/// Returns `Err(MathError::DivisionByZero)` if `total_assets == 0`. On first
/// deposit (empty pool), the caller should mint shares 1:1 with assets instead
/// of calling this function — dividing by zero total assets is a protocol
/// invariant violation, not just a math edge case.
pub fn assets_to_shares(
    assets: i128,
    total_shares: i128,
    total_assets: i128,
) -> Result<i128, MathError> {
    if total_assets == 0 {
        return Err(MathError::DivisionByZero);
    }
    safe_div(safe_mul(assets, total_shares)?, total_assets)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── shares_to_assets ────────────────────────────────────────────────────

    #[test]
    fn shares_to_assets_normal() {
        // 50 shares out of 100 total, 200 total assets → 100 assets
        assert_eq!(shares_to_assets(50, 100, 200), Ok(100));
    }

    #[test]
    fn shares_to_assets_all_shares() {
        // Holder of all shares gets all assets
        assert_eq!(shares_to_assets(1000, 1000, 5000), Ok(5000));
    }

    #[test]
    fn shares_to_assets_zero_shares_arg() {
        // 0 shares → 0 assets (pool is non-empty)
        assert_eq!(shares_to_assets(0, 100, 200), Ok(0));
    }

    #[test]
    fn shares_to_assets_zero_total_shares() {
        // Empty pool — denominator is zero, must NOT panic
        assert_eq!(
            shares_to_assets(10, 0, 200),
            Err(MathError::DivisionByZero)
        );
    }

    #[test]
    fn shares_to_assets_zero_total_assets() {
        // Pool exists (shares minted) but holds no assets — valid state, yields 0
        assert_eq!(shares_to_assets(50, 100, 0), Ok(0));
    }

    #[test]
    fn shares_to_assets_overflow() {
        // shares * total_assets overflows i128
        assert_eq!(
            shares_to_assets(i128::MAX, i128::MAX, i128::MAX),
            Err(MathError::Overflow)
        );
    }

    // ── assets_to_shares ────────────────────────────────────────────────────

    #[test]
    fn assets_to_shares_normal() {
        // Depositing 100 assets into a pool with 1000 shares and 200 assets
        // → 100 * 1000 / 200 = 500 shares
        assert_eq!(assets_to_shares(100, 1000, 200), Ok(500));
    }

    #[test]
    fn assets_to_shares_proportional() {
        // 1:1 pool — shares always equal assets
        assert_eq!(assets_to_shares(250, 1000, 1000), Ok(250));
    }

    #[test]
    fn assets_to_shares_zero_assets_arg() {
        assert_eq!(assets_to_shares(0, 1000, 500), Ok(0));
    }

    #[test]
    fn assets_to_shares_zero_total_assets() {
        // Empty-asset pool — must NOT panic, caller should mint 1:1 instead
        assert_eq!(
            assets_to_shares(100, 1000, 0),
            Err(MathError::DivisionByZero)
        );
    }

    #[test]
    fn assets_to_shares_zero_total_shares() {
        // No shares minted yet → result is 0 (0 * anything / denom = 0)
        assert_eq!(assets_to_shares(100, 0, 500), Ok(0));
    }

    #[test]
    fn assets_to_shares_overflow() {
        assert_eq!(
            assets_to_shares(i128::MAX, i128::MAX, 1),
            Err(MathError::Overflow)
        );
    }

    // ── safe_div existing behaviour ─────────────────────────────────────────

    #[test]
    fn safe_div_zero_denominator() {
        assert_eq!(safe_div(10, 0), Err(MathError::DivisionByZero));
    }

    #[test]
    fn safe_div_normal() {
        assert_eq!(safe_div(100, 4), Ok(25));
    }

    // ── calculate_percentage ────────────────────────────────────────────────

    #[test]
    fn calculate_percentage_normal() {
        // 5% of 1000 = 50
        assert_eq!(calculate_percentage(1000, 500), Ok(50));
    }

    #[test]
    fn calculate_percentage_zero_bps() {
        assert_eq!(calculate_percentage(1000, 0), Ok(0));
    }

    #[test]
    fn calculate_percentage_full_bps() {
        // 100% of 1000 = 1000
        assert_eq!(calculate_percentage(1000, 10_000), Ok(1000));
    }

    #[test]
    fn calculate_percentage_negative_bps() {
        assert_eq!(
            calculate_percentage(1000, -1),
            Err(MathError::Overflow)
        );
    }

    #[test]
    fn calculate_percentage_exceeds_bps() {
        assert_eq!(
            calculate_percentage(1000, 10_001),
            Err(MathError::Overflow)
        );
    }

    // ── apply_fee ────────────────────────────────────────────────────────────

    #[test]
    fn apply_fee_normal() {
        // 0.5% fee on 1000 → fee=5, net=995
        let (net, fee) = apply_fee(1000, 50).unwrap();
        assert_eq!(fee, 5);
        assert_eq!(net, 995);
    }

    #[test]
    fn apply_fee_zero_fee() {
        let (net, fee) = apply_fee(1000, 0).unwrap();
        assert_eq!(fee, 0);
        assert_eq!(net, 1000);
    }
}
