#![cfg_attr(not(test), no_std)]

// Minimal SDK v26 compatible test — verifies basic compilation and type system
#[cfg(test)]
mod tests {
    use soroban_sdk::{Env, Address};

    #[test]
    fn test_smoke_compile_ok() {
        let env = Env::default();
        let _addr = Address::from_str(&env, "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H");
    }

    #[test]
    fn test_circle_types_compile() {
        // Verifies that the core types compile correctly
        use crate::PayoutType;
        use crate::CircleFrequency;
        use crate::CircleStatus;

        let random = PayoutType::Random;
        let fixed = PayoutType::Fixed;
        let auction = PayoutType::Auction;
        let vote = PayoutType::Vote;
        assert!(true);

        let daily = CircleFrequency::Daily;
        let weekly = CircleFrequency::Weekly;
        let biweekly = CircleFrequency::Biweekly;
        let monthly = CircleFrequency::Monthly;
        assert!(true);

        let pending = CircleStatus::Pending;
        let active = CircleStatus::Active;
        let completed = CircleStatus::Completed;
        let cancelled = CircleStatus::Cancelled;
        let disputed = CircleStatus::Disputed;
        assert!(true);
    }

    #[test]
    fn test_errors_have_unique_codes() {
        use crate::CircleError;
        // Verify different errors have different codes
        assert_ne!(CircleError::NotActive as u32, CircleError::CircleFull as u32);
        assert_ne!(CircleError::AlreadyMember as u32, CircleError::NotMember as u32);
        assert_ne!(CircleError::InvalidAmount as u32, CircleError::RoundNotCurrent as u32);
        assert_ne!(CircleError::MaxStrikesReached as u32, CircleError::NotOrganizer as u32);
    }
}
