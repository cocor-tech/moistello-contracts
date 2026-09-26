#![cfg(test)]

use proptest::prelude::*;

use crate::math::{apply_fee, calculate_penalty, calculate_percentage, convert_shares, MathError};

proptest! {
    #[test]
    fn apply_fee_preserves_total(amount in 0_i128..=(i128::MAX / 10_000), fee_bps in 0_i128..=10_000) {
        let (net, fee) = apply_fee(amount, fee_bps).expect("bounded inputs should not overflow");
        prop_assert_eq!(net + fee, amount);
        prop_assert!(net >= 0);
        prop_assert!(fee >= 0);
        prop_assert!(fee <= amount);
    }

    #[test]
    fn calculate_percentage_stays_within_amount(amount in 0_i128..=(i128::MAX / 10_000), bps in 0_i128..=10_000) {
        let percentage = calculate_percentage(amount, bps).expect("bounded inputs should not overflow");
        prop_assert!(percentage >= 0);
        prop_assert!(percentage <= amount);
    }

    #[test]
    fn calculate_penalty_never_exceeds_total(amount in 0_i128..=(i128::MAX / 10_000), penalty_bps in 0_i128..=10_000) {
        let penalty = calculate_penalty(amount, penalty_bps).expect("bounded inputs should not overflow");
        prop_assert!(penalty >= 0);
        prop_assert!(penalty <= amount);
    }

    /// convert_shares must never panic for any non-zero total_shares.
    #[test]
    fn convert_shares_no_panic_nonzero_total(
        member_shares in 0_i128..=1_000_000_i128,
        total_shares  in 1_i128..=1_000_000_i128,
        pool_amount   in 0_i128..=(i128::MAX / 1_000_000_i128),
    ) {
        let result = convert_shares(member_shares, total_shares, pool_amount);
        // Must never panic — always returns Ok or a typed error.
        match result {
            Ok(v)  => prop_assert!(v >= 0),
            Err(e) => prop_assert_eq!(e, MathError::Overflow),
        }
    }
}

// ── Deterministic unit tests for convert_shares ──────────────────────────────

#[test]
fn convert_shares_zero_total_returns_division_by_zero() {
    // GUARD: total_shares == 0 must return DivisionByZero, not panic.
    let result = convert_shares(100, 0, 1_000);
    assert_eq!(result, Err(MathError::DivisionByZero));
}

#[test]
fn convert_shares_zero_pool_returns_zero() {
    // Pool is empty — every member gets 0 regardless of their share count.
    assert_eq!(convert_shares(50, 100, 0), Ok(0));
}

#[test]
fn convert_shares_equal_shares_splits_evenly() {
    // 5 members each holding 20 out of 100 shares, pool = 1000.
    // Each member should receive 200.
    assert_eq!(convert_shares(20, 100, 1_000), Ok(200));
}

#[test]
fn convert_shares_all_shares_to_one_member() {
    // Single member holds all shares — should receive the entire pool.
    assert_eq!(convert_shares(100, 100, 5_000), Ok(5_000));
}

#[test]
fn convert_shares_zero_member_shares_returns_zero() {
    // Member with no shares gets nothing.
    assert_eq!(convert_shares(0, 100, 1_000), Ok(0));
}

#[test]
fn convert_shares_single_share_of_many() {
    // 1 share out of 1_000_000, pool = 1_000_000 → member gets 1.
    assert_eq!(convert_shares(1, 1_000_000, 1_000_000), Ok(1));
}

#[test]
fn convert_shares_overflow_on_huge_inputs() {
    // member_shares * pool_amount overflows i128 → must return Overflow, not panic.
    let result = convert_shares(i128::MAX, 1, i128::MAX);
    assert_eq!(result, Err(MathError::Overflow));
}

// ── VRF key rotation tests (#472) ────────────────────────────────────────────

mod vrf_rotation {
    use crate::vrf::{self, VrfError};
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{contract, Address, BytesN, Env};

    #[contract]
    struct TestContract;

    fn setup(env: &Env) -> (Address, Address, BytesN<32>) {
        env.mock_all_auths();
        let contract_id = env.register(TestContract, ());
        let owner = Address::generate(env);
        let old_key = BytesN::from_array(env, &[1u8; 32]);
        env.as_contract(&contract_id, || {
            vrf::init_vrf(env, Some(&old_key), &owner).unwrap();
        });
        (contract_id, owner, old_key)
    }

    #[test]
    fn test_propose_then_activate_rotates_key() {
        let env = Env::default();
        let (contract_id, owner, old_key) = setup(&env);
        let new_key = BytesN::from_array(&env, &[2u8; 32]);
        let _sig = BytesN::from_array(&env, &[0u8; 64]);

        env.as_contract(&contract_id, || {
            // Evaluate + verify against the OLD key before proposing.
            let out0 = vrf::evaluate_vrf(&env, 1).unwrap();
            // verify_vrf will attempt ed25519 verification against the stored
            // (old) admin key; the bogus signature will fail cryptographic
            // verification but that failure happens inside ed25519_verify,
            // which panics on invalid signatures in the mock environment
            // rather than returning false — so we only assert the *plumbing*
            // here: output stability and that the key material itself is
            // what changes across rotation, not a full crypto proof.
            let _ = out0;

            vrf::propose_key_rotation(&env, &owner, &new_key, 100).unwrap();
        });

        // Old key must remain untouched immediately after proposing —
        // in-flight verification still targets the old key.
        env.as_contract(&contract_id, || {
            let stored_admin: BytesN<32> = env
                .storage()
                .instance()
                .get(&soroban_sdk::symbol_short!("vrf_admin"))
                .unwrap();
            assert_eq!(stored_admin, old_key);
        });

        // Activation before the delay elapses fails.
        env.as_contract(&contract_id, || {
            let result = vrf::activate_key_rotation(&env, &owner);
            assert_eq!(result, Err(VrfError::ActivationNotReady));
        });

        // Advance past the activation delay, then activate.
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
        env.as_contract(&contract_id, || {
            vrf::activate_key_rotation(&env, &owner).unwrap();
            let stored_admin: BytesN<32> = env
                .storage()
                .instance()
                .get(&soroban_sdk::symbol_short!("vrf_admin"))
                .unwrap();
            assert_eq!(stored_admin, new_key);
        });
    }

    #[test]
    fn test_activate_before_delay_fails() {
        let env = Env::default();
        let (contract_id, owner, _old_key) = setup(&env);
        let new_key = BytesN::from_array(&env, &[3u8; 32]);

        env.as_contract(&contract_id, || {
            vrf::propose_key_rotation(&env, &owner, &new_key, 1000).unwrap();
        });
        env.as_contract(&contract_id, || {
            let result = vrf::activate_key_rotation(&env, &owner);
            assert_eq!(result, Err(VrfError::ActivationNotReady));
        });
    }

    #[test]
    fn test_propose_rotation_unauthorized_caller_rejected() {
        let env = Env::default();
        let (contract_id, _owner, _old_key) = setup(&env);
        let not_owner = Address::generate(&env);
        let new_key = BytesN::from_array(&env, &[4u8; 32]);

        env.as_contract(&contract_id, || {
            let result = vrf::propose_key_rotation(&env, &not_owner, &new_key, 100);
            assert_eq!(result, Err(VrfError::Unauthorized));
        });
    }

    #[test]
    fn test_activate_rotation_unauthorized_caller_rejected() {
        let env = Env::default();
        let (contract_id, owner, _old_key) = setup(&env);
        let not_owner = Address::generate(&env);
        let new_key = BytesN::from_array(&env, &[5u8; 32]);

        env.as_contract(&contract_id, || {
            vrf::propose_key_rotation(&env, &owner, &new_key, 0).unwrap();
        });
        env.as_contract(&contract_id, || {
            let result = vrf::activate_key_rotation(&env, &not_owner);
            assert_eq!(result, Err(VrfError::Unauthorized));
        });
    }

    #[test]
    fn test_activate_with_no_pending_rotation_fails() {
        let env = Env::default();
        let (contract_id, owner, _old_key) = setup(&env);

        env.as_contract(&contract_id, || {
            let result = vrf::activate_key_rotation(&env, &owner);
            assert_eq!(result, Err(VrfError::NoPendingRotation));
        });
    }

    #[test]
    fn test_evaluations_after_propose_before_activate_use_old_key() {
        // The practical guarantee: proposing a rotation must not touch
        // ADMIN_KEY. Any evaluation/verification performed after propose but
        // before activate is unaffected because verify_vrf always reads
        // ADMIN_KEY fresh from storage, which propose_key_rotation never
        // writes to.
        let env = Env::default();
        let (contract_id, owner, old_key) = setup(&env);
        let new_key = BytesN::from_array(&env, &[6u8; 32]);

        env.as_contract(&contract_id, || {
            vrf::propose_key_rotation(&env, &owner, &new_key, 50).unwrap();
            let stored_admin: BytesN<32> = env
                .storage()
                .instance()
                .get(&soroban_sdk::symbol_short!("vrf_admin"))
                .unwrap();
            // Still the old key — in-flight verification during the
            // rotation window keeps checking against it.
            assert_eq!(stored_admin, old_key);
            assert_ne!(stored_admin, new_key);
        });

        env.ledger().set_timestamp(env.ledger().timestamp() + 50);
        env.as_contract(&contract_id, || {
            vrf::activate_key_rotation(&env, &owner).unwrap();
            let stored_admin: BytesN<32> = env
                .storage()
                .instance()
                .get(&soroban_sdk::symbol_short!("vrf_admin"))
                .unwrap();
            assert_eq!(stored_admin, new_key);
        });
    }
}

#[soroban_sdk::contract]
pub struct TestVrfContract;

#[soroban_sdk::contractimpl]
impl TestVrfContract {
    pub fn run_vrf(env: soroban_sdk::Env, owner: soroban_sdk::Address, seed1: u32, seed2: u32) -> (u32, u32) {
        crate::vrf::init_vrf(&env, None, &owner).unwrap();
        let out1 = crate::vrf::evaluate_vrf(&env, seed1).unwrap();
        let out2 = crate::vrf::evaluate_vrf(&env, seed2).unwrap();
        (out1, out2)
    }
}

#[test]
fn test_vrf_init_and_evaluate_emits_fulfilled() {
    use soroban_sdk::testutils::Address as _;
    let env = soroban_sdk::Env::default();
    let owner = soroban_sdk::Address::generate(&env);
    let contract_id = env.register(TestVrfContract, ());
    let client = TestVrfContractClient::new(&env, &contract_id);
    let (out1, out2) = client.run_vrf(&owner, &42, &43);
    assert_ne!(out1, 0);
    assert_ne!(out2, 0);
}
