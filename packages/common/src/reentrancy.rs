use soroban_sdk::{Env, Symbol, contracterror, symbol_short};

const REENTRANCY_KEY: Symbol = symbol_short!("reent");

/// ReentrancyGuard prevents recursive calls to the same function.
///
/// Uses **temporary** storage (not instance/persistent) so that if a
/// transaction panics or runs out of gas after acquiring the lock, the
/// flag auto-expires after the temporary TTL elapses, preventing a
/// permanent contract lock. A short TTL (2 ledgers) is set explicitly
/// so the contract is usable again within seconds.
///
/// Usage:
///   let guard = ReentrancyGuard::new(&env)?;
///   // ... mutating logic ...
///   drop(guard); // or let it go out of scope
pub struct ReentrancyGuard {
    env: Env,
}

impl ReentrancyGuard {
    /// Acquires the reentrancy lock. Returns an error if already locked.
    pub fn new(env: &Env) -> Result<Self, ReentrancyError> {
        let locked: bool = env.storage().temporary().get(&REENTRANCY_KEY).unwrap_or(false);
        if locked {
            return Err(ReentrancyError::ReentrantCall);
        }
        env.storage().temporary().set(&REENTRANCY_KEY, &true);
        // Extend TTL to 2 ledgers so the key survives the current tx but
        // auto-expires quickly if the tx panics before drop() runs.
        env.storage().temporary().extend_ttl(&REENTRANCY_KEY, 1, 2);
        Ok(Self { env: env.clone() })
    }
}

impl Drop for ReentrancyGuard {
    fn drop(&mut self) {
        self.env.storage().temporary().set(&REENTRANCY_KEY, &false);
    }
}

#[contracterror]
#[derive(Debug)]
pub enum ReentrancyError {
    ReentrantCall = 1,
}
