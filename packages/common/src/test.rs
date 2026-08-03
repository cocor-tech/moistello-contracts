#![cfg(test)]

use proptest::prelude::*;

use crate::math::{apply_fee, calculate_penalty, calculate_percentage};

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
}
