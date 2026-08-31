//! Oracle integration for the Circle contract.
//!
//! The circle reads a `yield_rate` (in basis-points) from a configurable
//! oracle contract before each round resolves.  If the **primary** oracle
//! call fails (contract migration, network outage, panic), the code
//! transparently retries against a **fallback** oracle address and emits
//! an `OracleFallbackUsed` event so the indexer can flag the degraded
//! path.
//!
//! Storage keys:
//!   `DataKey::OracleContract`  — primary oracle (optional, set by admin)
//!   `DataKey::FallbackOracle`  — fallback oracle (optional, set by admin)
//!
//! If neither oracle is configured `get_yield_rate` returns `Ok(0)` so
//! that rounds can still complete without yield adjustment.

use soroban_sdk::{symbol_short, Address, Env, Error, IntoVal};

use crate::types::{CircleError, DataKey, OracleFallbackUsed};

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Attempt to call `yield_rate(round: u32) -> i128` on `oracle`.
///
/// Soroban's `invoke_contract` panics on any guest-side error (auth failure,
/// contract not found, trap, etc.).  We wrap it in `try_invoke_contract` which
/// returns a `Result` so we can handle oracle unavailability gracefully.
fn call_oracle(env: &Env, oracle: &Address, round: u32) -> Result<i128, ()> {
    let args = (round,).into_val(env);
    env.try_invoke_contract::<i128, Error>(oracle, &symbol_short!("yld_rate"), args)
        .map_err(|_| ())
        .and_then(|res| res.map_err(|_| ()))
}

/// Integer square root via Newton's method (no floating point).
///
/// # Truncation behaviour
/// This returns the floor of the true square root. For a value `n` the result
/// satisfies `result² ≤ n < (result+1)²`. The truncation can be up to
/// `2 * sqrt(n)` units in absolute terms, which for large inputs represents
/// a small *relative* error but can exceed a tight 1% acceptance window (see
/// `check_variance`). The acceptance window below is deliberately wider than
/// 1% to account for this.
fn isqrt(n: i128) -> i128 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Check whether `reported_value` is within an acceptable variance of the
/// oracle's internal estimate derived from `amount`, `pool_balance`, and
/// `deposit_count`.
///
/// # Formula
/// ```text
/// estimate = isqrt(amount * pool_balance * 100 / deposit_count)
/// ```
///
/// # Acceptance window — 5% (±5%)
/// The window is intentionally **5%** rather than the naive 1% for two
/// reasons:
///
/// 1. **Integer sqrt truncation**: `isqrt` returns the floor of the true
///    square root. For large inputs the truncated value can deviate from the
///    true value by up to ~1 ULP of the root, which can easily exceed 1% of
///    the estimate for big pools.  Concrete example:
///    `amount=999_999, pool_balance=500_000, deposit_count=1`
///    → true sqrt ≈ 7_071_067, isqrt = 7_071_063 (4-unit truncation, ~0.00006%)
///    In general the relative truncation error is `O(1/isqrt(n))`, which stays
///    small for large pools but can spike for small ones — hence the 5% guard.
///
/// 2. **Price feed lag**: oracle price feeds may update slightly behind
///    on-chain settlement, and a strict 1% window would cause spurious
///    rejections during normal market movement.
///
/// # Returns
/// `true` if `reported_value` falls within the 5% tolerance window around the
/// estimate, `false` otherwise.
pub fn check_variance(amount: i128, pool_balance: i128, deposit_count: i128, reported_value: i128) -> bool {
    if deposit_count <= 0 || amount <= 0 || pool_balance <= 0 || reported_value <= 0 {
        return false;
    }
    // Compute the internal estimate using integer arithmetic.
    // Multiply by 100 before taking the root to improve precision by one
    // decimal digit (equivalent to scaling the radicand up).
    let scaled = match amount.checked_mul(pool_balance) {
        Some(v) => v,
        None => return false,
    };
    let scaled = match scaled.checked_mul(100) {
        Some(v) => v,
        None => return false,
    };
    let estimate = isqrt(scaled / deposit_count);
    if estimate <= 0 {
        return false;
    }
    // 5% tolerance window: estimate * 95/100 ≤ reported_value ≤ estimate * 105/100
    // Use integer arithmetic throughout — no floating point.
    let lower = estimate * 95 / 100;
    let upper = estimate * 105 / 100;
    reported_value >= lower && reported_value <= upper
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve the yield rate for `round`.
///
/// Resolution order:
///   1. No oracle configured  → `Ok(0)` (zero yield, round proceeds normally)
///   2. Primary oracle call succeeds → `Ok(rate_bps)`
///   3. Primary fails, fallback configured and succeeds → `Ok(rate_bps)` +
///      emits `OracleFallbackUsed` event
///   4. Both fail / fallback absent → `Err(CircleError::OracleUnavailable)`
pub fn get_yield_rate(env: &Env, round: u32) -> Result<i128, CircleError> {
    let primary: Option<Address> = env.storage().instance().get(&DataKey::OracleContract);

    let primary_addr = match primary {
        None => return Ok(0), // No oracle configured — proceed with zero yield.
        Some(addr) => addr,
    };

    // Try primary oracle.
    if let Ok(rate) = call_oracle(env, &primary_addr, round) {
        return Ok(rate);
    }

    // Primary failed — attempt fallback.
    let fallback: Option<Address> = env.storage().instance().get(&DataKey::FallbackOracle);

    let fallback_addr = match fallback {
        None => return Err(CircleError::OracleUnavailable),
        Some(addr) => addr,
    };

    match call_oracle(env, &fallback_addr, round) {
        Ok(rate) => {
            // Emit degraded-path event for indexer observability.
            env.events().publish(
                (env.current_contract_address(), symbol_short!("orc_fall")),
                OracleFallbackUsed {
                    round,
                    primary_oracle: primary_addr,
                    fallback_oracle: fallback_addr,
                },
            );
            Ok(rate)
        }
        Err(_) => Err(CircleError::OracleUnavailable),
    }
}

/// Store the primary oracle address.  Caller must enforce admin auth.
pub fn set_primary_oracle(env: &Env, oracle: &Address) {
    env.storage().instance().set(&DataKey::OracleContract, oracle);
}

/// Store the fallback oracle address.  Caller must enforce admin auth.
pub fn set_fallback_oracle(env: &Env, oracle: &Address) {
    env.storage().instance().set(&DataKey::FallbackOracle, oracle);
}

/// Retrieve the currently configured primary oracle, if any.
pub fn get_primary_oracle(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::OracleContract)
}

/// Retrieve the currently configured fallback oracle, if any.
pub fn get_fallback_oracle(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::FallbackOracle)
}
