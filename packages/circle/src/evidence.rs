//! Dispute evidence commitments (issue #340).
//!
//! # The problem
//!
//! `raise_dispute` has always stored an `evidence_hash`, but nothing tied that
//! commitment to real evidence, so an admin resolved disputes on trust. The
//! contract could not answer the only question that matters: "does the evidence
//! the raiser is pointing at actually hash to the value recorded on-chain?"
//!
//! # The scheme
//!
//! 1. The raiser publishes the evidence off-chain (IPFS, a URL, a signed
//!    document) and commits to it on-chain with `sha256(evidence)`.
//! 2. `raise_dispute` rejects an all-zero commitment, so every dispute carries a
//!    real commitment.
//! 3. Anyone can call `verify_evidence(data)` with the candidate preimage. The
//!    contract re-hashes the bytes and compares them to the stored commitment,
//!    so a verifier does not have to trust the raiser or the admin.
//! 4. `resolve_dispute_with_evidence(resolution, data)` makes the same check a
//!    precondition of resolution: the admin must supply the preimage that
//!    matches the commitment before the circle status can change.
//!
//! Only the 32-byte digest is ever stored, so dispute evidence does not consume
//! proportional ledger space with the size of the evidence itself.

use soroban_sdk::{Bytes, BytesN, Env};

/// The 32-byte digest of an all-zero commitment, used to detect "no evidence".
pub fn empty_commitment(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

/// True when `commitment` is an all-zero digest, i.e. no evidence was supplied.
pub fn is_empty_commitment(env: &Env, commitment: &BytesN<32>) -> bool {
    commitment.to_array() == empty_commitment(env).to_array()
}

/// `sha256(evidence)` — the on-chain commitment for a piece of dispute evidence.
pub fn hash(env: &Env, evidence: &Bytes) -> BytesN<32> {
    BytesN::from(env.crypto().sha256(evidence))
}

/// True when `sha256(evidence)` equals the stored `commitment`.
pub fn matches(env: &Env, evidence: &Bytes, commitment: &BytesN<32>) -> bool {
    hash(env, evidence).to_array() == commitment.to_array()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_commitment_is_detected() {
        let env = Env::default();
        assert!(is_empty_commitment(&env, &empty_commitment(&env)));
        assert!(!is_empty_commitment(
            &env,
            &BytesN::from_array(&env, &[1u8; 32])
        ));
    }

    #[test]
    fn hash_is_deterministic_and_matches() {
        let env = Env::default();
        let evidence = Bytes::from_slice(&env, b"ledger export + chat log");
        let commitment = hash(&env, &evidence);
        assert!(!is_empty_commitment(&env, &commitment));
        assert!(matches(&env, &evidence, &commitment));
    }

    #[test]
    fn different_evidence_does_not_match() {
        let env = Env::default();
        let commitment = hash(&env, &Bytes::from_slice(&env, b"evidence-a"));
        assert!(!matches(&env, &Bytes::from_slice(&env, b"evidence-b"), &commitment));
    }

    #[test]
    fn tampered_evidence_does_not_match() {
        let env = Env::default();
        let original = Bytes::from_slice(&env, b"amount=100");
        let commitment = hash(&env, &original);
        assert!(!matches(&env, &Bytes::from_slice(&env, b"amount=900"), &commitment));
    }
}
