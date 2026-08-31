#![cfg(test)]

use proptest::prelude::*;
use soroban_sdk::Env;

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

#[soroban_sdk::contract]
pub struct DummyContract;

#[soroban_sdk::contractimpl]
impl DummyContract {
    pub fn dummy(_env: Env) {}
}

#[test]
fn test_upgrade_contract_zero_hash_fails() {
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};
    use crate::upgrade::{upgrade_contract, UpgradeError};

    let env = Env::default();
    let dummy_contract = env.register(DummyContract, ());
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    env.as_contract(&dummy_contract, || {
        let res = upgrade_contract(&env, &admin, &zero_hash);
        assert_eq!(res, Err(UpgradeError::InvalidWasmHash));
    });
}

#[test]
fn test_vrf_hash_chain_only_mode() {
    use soroban_sdk::{BytesN, Env};
    use crate::vrf::{init_vrf, evaluate_vrf, verify_vrf};

    let env = Env::default();
    let dummy_contract = env.register(DummyContract, ());
    env.mock_all_auths();

    env.as_contract(&dummy_contract, || {
        // init with None admin key
        assert!(init_vrf(&env, None).is_ok());

        let val = evaluate_vrf(&env, 42).expect("evaluate_vrf should succeed");
        let sig = BytesN::from_array(&env, &[0u8; 64]);

        // verify_vrf returns false in hash-chain-only mode (no admin key configured)
        assert!(!verify_vrf(&env, 42, val, 0, &sig));
    });
}
