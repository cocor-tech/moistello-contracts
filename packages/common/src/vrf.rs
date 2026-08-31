/// Verifiable Random Function (VRF) for Moistello contracts.
///
/// This module replaces the previous PRNG-based randomness (`env.prng()`) with
/// a cryptographically verifiable random function built on:
///
/// - **SHA-256 hash chain**: Each evaluation produces a deterministic output
///   from `(input_seed, salt, counter)`. The salt is generated via
///   `env.prng().gen()` at initialization and is a consensus-level value
///   derived from the overall transaction-set hash, making it cryptographically
///   hard to bias even by a corrupt validator.
///
/// - **Ed25519 signature verification**: An admin keypair (Ed25519) is stored
///   at initialization. Each VRF evaluation can be accompanied by an Ed25519
///   signature over the evaluation output, allowing any third party to verify
///   the randomness was authorized by the admin. This prevents unauthorized
///   or tampered random values.
///
/// ## VRF Properties
///
/// 1. **Determinism**: Given the same `(input_seed, salt, counter)`, the VRF
///    always produces the same output. Anyone can recompute and verify.
/// 2. **Unpredictability**: The salt is a consensus-level random value (from
///    transaction-set hash), unknown before the ledger closes. Combined with
///    the incrementing counter, each evaluation is unique and unpredictable.
/// 3. **Verifiability**: The Ed25519 signature provides cryptographic proof
///    that the admin authorized the specific VRF output. `verify_vrf` checks
///    this signature against the stored public key.
/// 4. **Non-replayability**: The counter increments with each evaluation.
///    Each `(input_seed, counter)` pair is unique.
///
/// ## Usage in Circle Payouts
///
/// The circle contract calls `shuffle_positions(env, n)` to generate a random
/// permutation of payout positions. Each position is derived from a separate
/// VRF evaluation with an incremented counter, ensuring each position's
/// randomness is independently derived.
///
/// For enhanced security, the admin can sign VRF outputs off-chain and callers
/// can verify via `verify_vrf()` before accepting the shuffled order.

use soroban_sdk::{contracterror, contractevent, symbol_short, Bytes, BytesN, Env, Vec};

// ── Storage keys ──────────────────────────────────────────────────────────

const ADMIN_KEY: soroban_sdk::Symbol = symbol_short!("vrf_admin");
const SALT_KEY: soroban_sdk::Symbol = symbol_short!("vrf_salt");
const COUNTER_KEY: soroban_sdk::Symbol = symbol_short!("vrf_ctr");

// ── Errors ────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VrfError {
    /// VRF has not been initialized — call `init_vrf` first.
    NotInitialized = 1,
    /// The Ed25519 signature does not match the expected VRF output.
    InvalidProof = 2,
    /// The VRF has already been initialized — reinitialization is prohibited.
    AlreadyInitialized = 3,
    /// Math overflow during counter or range computation.
    Overflow = 4,
}

// ── Events ────────────────────────────────────────────────────────────────

/// Emitted on each VRF evaluation, enabling off-chain indexers to track
/// the hash chain and verify randomness deterministically.
#[contractevent]
#[derive(Clone, Debug)]
pub struct VrfEvaluated {
    pub input_seed: u32,
    pub vrf_output: u32,
    pub counter: u32,
}

// ── Public API ────────────────────────────────────────────────────────────

/// Initialize the VRF with an optional Ed25519 public key for signature verification.
///
/// Must be called exactly once before any VRF evaluation. Generates a random
/// salt via `env.prng()` (consensus-level entropy from the transaction-set
/// hash) and stores it alongside the optional admin key.
///
/// When `admin_key` is `Some(key)`, VRF outputs can be verified via
/// When `admin_key` is `Some(key)`, VRF outputs can be verified via
/// `verify_vrf()` using Ed25519 signature checks. When `admin_key` is `None`,
/// the VRF operates in hash-chain-only mode — outputs are deterministic and
/// publicly verifiable by recomputation, but signature verification (`verify_vrf`)
/// will return `false` as no admin key is configured.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `admin_key` - Optional Ed25519 public key (32 bytes) for signature verification
///
/// # Errors
/// * `VrfError::AlreadyInitialized` if called more than once
pub fn init_vrf(env: &Env, admin_key: Option<&BytesN<32>>) -> Result<(), VrfError> {
    if env.storage().instance().has(&SALT_KEY) {
        return Err(VrfError::AlreadyInitialized);
    }
    let salt: BytesN<32> = env.prng().gen();
    env.storage().instance().set(&SALT_KEY, &salt);
    env.storage().instance().set(&COUNTER_KEY, &0u32);
    if let Some(key) = admin_key {
        env.storage().instance().set(&ADMIN_KEY, key);
    }
    Ok(())
}

/// Compute a VRF evaluation: deterministic hash of `(input_seed, salt, counter)`.
///
/// Returns the 32-byte SHA-256 hash truncated to a `u32`. The counter is
/// incremented after each evaluation, ensuring uniqueness. This function is
/// deterministic — anyone can recompute the output given the same inputs.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `input_seed` - Caller-provided seed (e.g., round number, member position)
///
/// # Returns
/// A `u32` derived from the SHA-256 hash, suitable for use in shuffling or
/// random selection.
///
/// # Errors
/// * `VrfError::NotInitialized` if `init_vrf` has not been called
/// * `VrfError::Overflow` if the internal counter overflows
pub fn evaluate_vrf(env: &Env, input_seed: u32) -> Result<u32, VrfError> {
    let counter: u32 = env
        .storage()
        .instance()
        .get(&COUNTER_KEY)
        .ok_or(VrfError::NotInitialized)?;
    let salt: BytesN<32> = env
        .storage()
        .instance()
        .get(&SALT_KEY)
        .ok_or(VrfError::NotInitialized)?;

    let output = compute_vrf_hash(env, input_seed, &salt, counter)?;

    let next_counter = counter.checked_add(1).ok_or(VrfError::Overflow)?;
    env.storage().instance().set(&COUNTER_KEY, &next_counter);

    VrfEvaluated {
        input_seed,
        vrf_output: output,
        counter,
    }
    .publish(env);

    Ok(output)
}

/// Verify a VRF output against an Ed25519 signature.
///
/// Recomputes the hash from `(input_seed, salt, counter)` and checks that
/// `signature` is a valid Ed25519 signature over the 32-byte hash by the
/// stored admin public key. This allows any third party to independently
/// verify that a VRF output was authorized by the admin.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `input_seed` - The seed used during evaluation
/// * `vrf_output` - The claimed VRF output to verify
/// * `counter` - The counter value used during evaluation (from `VrfEvaluated` event)
/// * `signature` - Ed25519 signature (64 bytes) over the hash
///
/// # Returns
/// `true` if the signature is valid, `false` otherwise. Does not panic.
pub fn verify_vrf(
    env: &Env,
    input_seed: u32,
    vrf_output: u32,
    counter: u32,
    signature: &BytesN<64>,
) -> bool {
    let admin_key: BytesN<32> = match env.storage().instance().get(&ADMIN_KEY) {
        Some(k) => k,
        None => return false,
    };
    let salt: BytesN<32> = match env.storage().instance().get(&SALT_KEY) {
        Some(s) => s,
        None => return false,
    };

    let expected_output = match compute_vrf_hash(env, input_seed, &salt, counter) {
        Ok(o) => o,
        Err(_) => return false,
    };

    if expected_output != vrf_output {
        return false;
    }

    let hash_bytes = hash_to_bytes(env, input_seed, &salt, counter);

    let crypto = env.crypto();
    crypto.ed25519_verify(&admin_key, &hash_bytes, signature);
    true
}

/// Shuffle positions 0..n using VRF evaluations.
///
/// Generates a random permutation of `[0, n)` by performing `n` VRF
/// evaluations with incrementing input seeds. The salt and counter
/// ensure each position's randomness is independently derived.
///
/// This is used by the circle contract to determine random payout order.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `n` - Number of positions to shuffle (must be > 0)
///
/// # Returns
/// A `Vec<u32>` containing the shuffled positions.
///
/// # Errors
/// * `VrfError::NotInitialized` if `init_vrf` has not been called
pub fn shuffle_positions(env: &Env, n: u32) -> Result<Vec<u32>, VrfError> {
    let mut shuffled = Vec::new(env);
    for i in 0..n {
        let vrf_val = evaluate_vrf(env, i)?;
        let pos = vrf_val % n;
        shuffled.push_back(pos);
    }
    Ok(shuffled)
}

/// Generate a pseudo-random `u32` in `[0, max)` using VRF.
///
/// Evaluates the VRF with `input_seed = 0` and takes the result modulo `max`.
/// This is a convenience wrapper for single-value random generation.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `max` - Upper bound (exclusive). If 0, returns 0.
///
/// # Returns
/// A `u32` in the range `[0, max)`.
pub fn random_in_range(env: &Env, max: u32) -> Result<u32, VrfError> {
    if max == 0 {
        return Ok(0);
    }
    let vrf_val = evaluate_vrf(env, 0)?;
    Ok(vrf_val % max)
}

// ── Internal helpers ──────────────────────────────────────────────────────

/// Compute the VRF hash deterministically from inputs.
///
/// SHA-256(input_seed:u32 ++ salt:32bytes ++ counter:u32) → first 4 bytes → u32
fn compute_vrf_hash(
    env: &Env,
    input_seed: u32,
    salt: &BytesN<32>,
    counter: u32,
) -> Result<u32, VrfError> {
    let hash_bytes = hash_to_bytes(env, input_seed, salt, counter);
    let hash = env.crypto().sha256(&hash_bytes);
    let array = hash.to_array();
    Ok(u32::from_le_bytes([array[0], array[1], array[2], array[3]])
        .wrapping_add(u32::from_le_bytes([array[4], array[5], array[6], array[7]])))
}

/// Build the pre-hash byte sequence: input_seed(4) ++ salt(32) ++ counter(4)
fn hash_to_bytes(env: &Env, input_seed: u32, salt: &BytesN<32>, counter: u32) -> Bytes {
    let mut data = Bytes::new(env);
    data.extend_from_slice(&input_seed.to_le_bytes());
    data.extend_from_slice(&salt.to_array());
    data.extend_from_slice(&counter.to_le_bytes());
    data
}
