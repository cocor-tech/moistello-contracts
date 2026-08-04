#![cfg_attr(not(test), no_std)]

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

use soroban_sdk::{symbol_short, Address, Env, IntoVal};

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
    env.try_invoke_contract::<i128, _>(oracle, &symbol_short!("yld_rate"), args)
        .map_err(|_| ())
        .and_then(|res| res.map_err(|_| ()))
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
