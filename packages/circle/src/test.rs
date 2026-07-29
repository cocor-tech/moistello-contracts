#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests {
    use soroban_sdk::{Address, BytesN, Env, String};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger as _;
    use crate as circle;
<<<<<<< HEAD
    use circle::{Circle, CircleArgs, CircleStatus};
=======
    use circle::CircleError;

    const MEMBER_ACTIVE: u32 = 0u32;
>>>>>>> master

    fn create_config(env: &Env) -> circle::types::CircleConfig {
        circle::types::CircleConfig {
            organizer: Address::generate(env),
            name: String::from_str(env, "Test Circle"),
            contribution_amount: 100_0000000i128,
            max_members: 5u32,
            payout_type: 0u32,
            total_rounds: 5u32,
            contribution_deadline_seconds: 604800u64,
            min_moi_score: 0u32,
            collateral_amount: 0i128,
            penalty_bps: 500u32,
            grace_period_seconds: 86400u64,
            max_strikes: 3u32,
            slug: String::from_str(env, "test-circle"),
            fee_bps: 0u32,
        }
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let status = client.get_status();
        assert_eq!(status.status, CircleStatus::Pending);
    }

    #[test]
    fn test_join() {
        let env = Env::default();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);

        env.mock_all_auths();
        assert!(client.try_join(&member).is_ok());
        assert_eq!(client.get_members().len(), 1);
    }

    #[test]
    fn test_join_full() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);

        env.mock_all_auths();
        client.try_join(&Address::generate(&env)).unwrap();
        client.try_join(&Address::generate(&env)).unwrap();
        assert!(client.try_join(&Address::generate(&env)).is_err());
    }

    #[test]
    fn test_duplicate_join() {
        let env = Env::default();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);

        env.mock_all_auths();
        assert!(client.try_join(&member).is_ok());
        assert!(client.try_join(&member).is_err());
    }

    #[test]
    fn test_contribute() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&member).unwrap();
        client.try_join(&other).unwrap();
        assert!(client.try_contribute(&member, &config.contribution_amount, &0u32).is_ok());
    }

    #[test]
    fn test_contribute_wrong_amount() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&member).unwrap();
        client.try_join(&other).unwrap();
        assert!(client.try_contribute(&member, &50_0000000i128, &0u32).is_err());
    }

    #[test]
    fn test_contribute_not_member() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let outsider = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&member).unwrap();
        client.try_join(&outsider).unwrap();
        let non_member = Address::generate(&env);
        assert!(client.try_contribute(&non_member, &config.contribution_amount, &0u32).is_err());
    }

    #[test]
    fn test_exit() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&member).unwrap();
        client.try_join(&other).unwrap();
        client.try_contribute(&member, &config.contribution_amount, &0u32).unwrap();
        assert!(client.try_exit_circle(&member).is_ok());
    }

    #[test]
    fn test_pause_unpause() {
        let env = Env::default();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);

        env.mock_all_auths();
        assert!(client.try_pause_circle(&admin).is_ok());
        let member = Address::generate(&env);
        assert!(client.try_join(&member).is_err());
        assert!(client.try_unpause_circle(&admin).is_ok());
        assert!(client.try_join(&member).is_ok());
    }

    #[test]
    fn test_empty_circle_get_members() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);
        let members = client.get_members();
        assert_eq!(members.len(), 0);
    }

    #[test]
    fn test_empty_circle_get_contributions() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);
        let member = Address::generate(&env);
        let contributions = client.get_contributions(&member);
        assert_eq!(contributions.len(), 0);
    }

    #[test]
    fn test_trigger_payout_not_active_when_pending() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        // Only 1 member joined — circle stays PENDING (not full)
        let m1 = Address::generate(&env);
        client.try_join(&m1).unwrap();
        let result = client.try_trigger_payout(&admin, &0u32);
        assert_eq!(result, Err(Ok(CircleError::NotActive)));
    }

    #[test]
    fn test_trigger_payout_after_all_members_exit() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.total_rounds = 3u32;
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();

        // Both exit — 0 active members remain
        client.try_exit_circle(&m1).unwrap();
        client.try_exit_circle(&m2).unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        assert_eq!(result, Err(Ok(CircleError::PayoutAlreadyExecuted)));
    }

    #[test]
    fn test_resolve_dispute_no_dispute() {
        let env = Env::default();
        let mut config = create_config(&env);
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let result = client.try_resolve_dispute(&admin, &1u32);
        assert_eq!(result, Err(Ok(CircleError::NoActiveDispute)));
    }

    #[test]
    fn test_exit_from_empty_circle() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let stranger = Address::generate(&env);
        // Exiting from a circle with no members and no collateral succeeds
        // with no state change (no active member found, no-op)
        let result = client.try_exit_circle(&stranger);
        assert!(result.is_ok());
        assert_eq!(client.get_members().len(), 0);
    }

    #[test]
    fn test_exit_non_member_does_not_affect_state() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let real_member = Address::generate(&env);
        client.try_join(&real_member).unwrap();

        let stranger = Address::generate(&env);
        // Stranger exits — succeeds as no-op since stranger isn't in members list
        let result = client.try_exit_circle(&stranger);
        assert!(result.is_ok());
        // Real member should still be in the circle
        assert_eq!(client.get_members().len(), 1);
    }

    #[test]
    fn test_trigger_payout_fixed_no_active_members() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 1u32; // PAYOUT_FIXED
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();

        client.try_exit_circle(&m1).unwrap();
        client.try_exit_circle(&m2).unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        // resolve_fixed builds pos_to_addr from active members only,
        // finds no active member at the target position → NotMember
        assert_eq!(result, Err(Ok(CircleError::NotMember)));
    }

    #[test]
    fn test_trigger_payout_auction_no_active_members() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();

        client.try_exit_circle(&m1).unwrap();
        client.try_exit_circle(&m2).unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        // resolve_auction finds no bids → VoteQuorumNotMet
        assert_eq!(result, Err(Ok(CircleError::VoteQuorumNotMet)));
    }

    #[test]
    fn test_trigger_payout_vote_no_active_members() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 3u32; // PAYOUT_VOTE
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();

        client.try_exit_circle(&m1).unwrap();
        client.try_exit_circle(&m2).unwrap();

        let result = client.try_trigger_payout(&admin, &0u32);
        // resolve_vote finds 0 active members, quorum = 1, no votes → VoteQuorumNotMet
        assert_eq!(result, Err(Ok(CircleError::VoteQuorumNotMet)));
    }

    #[test]
    fn test_raise_dispute_on_empty_circle() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let member = Address::generate(&env);
        let evidence = BytesN::from_array(&env, &[0u8; 32]);
        let result = client.try_raise_dispute(&member, &evidence);
        // Circle is PENDING (not full) — but raise_dispute only checks for DISPUTED/COMPLETED status
        // So any member (even non-member) can raise a dispute on any circle
        assert!(result.is_ok());
    }

    #[test]
    fn test_contribute_fails_on_empty_contributions() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let (token, client) = setup_test_env(&env, &mut config);
        let m1 = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&m1).unwrap();

        mint_tokens(&env, &token, &m1, config.contribution_amount);
        let result = client.try_contribute(&m1, &config.contribution_amount, &0u32);
        // Should succeed since m1 is a member and contributions is empty
        assert!(result.is_ok());
        let contributions = client.get_contributions(&m1);
        assert_eq!(contributions.len(), 1);
    }

    #[test]
    fn test_unauthorized() {
        let env = Env::default();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);

        // No env.mock_all_auths() — should fail authorization
        assert!(client.try_join(&member).is_err());
    }

    #[test]
    fn test_fee_bps_applied_on_payout() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.fee_bps = 500u32; // 5%
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap();
        client.try_trigger_payout(&admin, &0u32).unwrap();

        let pool = config.contribution_amount * 2;
        let expected_fee = pool * 500 / 10_000;
        assert_eq!(client.get_status().total_fees, expected_fee);
        assert_eq!(client.get_status().total_payouts, pool - expected_fee);
    }

    #[test]
    fn test_set_fee_bps_updates_payout_fee() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        client.try_set_fee_bps(&admin, &1000u32).unwrap();
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap();
        client.try_trigger_payout(&admin, &0u32).unwrap();

        let pool = config.contribution_amount * 2;
        let expected_fee = pool * 1000 / 10_000;
        assert_eq!(client.get_status().total_fees, expected_fee);
    }

    #[test]
    fn test_full_lifecycle() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 3u32;
        config.total_rounds = 3u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);

        env.mock_all_auths();

        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        let m3 = Address::generate(&env);

        // Join
        assert!(client.try_join(&m1).is_ok());
        assert!(client.try_join(&m2).is_ok());
        assert!(client.try_join(&m3).is_ok());
        assert_eq!(client.get_members().len(), 3);

        // Round 0
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap();
        client.try_contribute(&m3, &config.contribution_amount, &0u32).unwrap();
        client.try_trigger_payout(&admin, &0u32).unwrap();
        assert_eq!(client.get_status().current_round, 1u32);

        // Round 1
        client.try_contribute(&m1, &config.contribution_amount, &1u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &1u32).unwrap();
        client.try_contribute(&m3, &config.contribution_amount, &1u32).unwrap();
        client.try_trigger_payout(&admin, &1u32).unwrap();

        // Round 2
        client.try_contribute(&m1, &config.contribution_amount, &2u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &2u32).unwrap();
        client.try_contribute(&m3, &config.contribution_amount, &2u32).unwrap();
        client.try_trigger_payout(&admin, &2u32).unwrap();

        // Should be completed
        assert_eq!(client.get_status().status, CircleStatus::Completed);
    }

    #[test]
    fn test_get_contributions() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&member).unwrap();
        client.try_join(&other).unwrap();
        client.try_contribute(&member, &config.contribution_amount, &0u32).unwrap();

        let contributions = client.get_contributions(&member);
        assert_eq!(contributions.len(), 1);
        assert_eq!(contributions.get(0).unwrap().member, member);
        assert_eq!(contributions.get(0).unwrap().round, 0u32);
    }

    #[test]
    fn test_auction_bid_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let bidder = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&bidder).unwrap();
        client.try_join(&other).unwrap();

        assert!(client.try_auction_bid(&bidder, &500u32, &0u32).is_ok());
    }

    #[test]
    fn test_auction_bid_duplicate() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let bidder = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&bidder).unwrap();
        client.try_join(&other).unwrap();

        client.try_auction_bid(&bidder, &500u32, &0u32).unwrap();
        assert!(client.try_auction_bid(&bidder, &600u32, &0u32).is_err());
    }

    #[test]
    fn test_auction_bid_invalid_discount() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 2u32; // PAYOUT_AUCTION
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let bidder = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&bidder).unwrap();
        client.try_join(&other).unwrap();

        assert!(client.try_auction_bid(&bidder, &10001u32, &0u32).is_err());
    }

    #[test]
    fn test_vote_payout_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 3u32; // PAYOUT_VOTE
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let voter = Address::generate(&env);
        let nominee = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&voter).unwrap();
        client.try_join(&nominee).unwrap();

        assert!(client.try_vote_payout(&voter, &nominee, &0u32).is_ok());
    }

    #[test]
    fn test_vote_payout_duplicate() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 3u32; // PAYOUT_VOTE
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let voter = Address::generate(&env);
        let nominee = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&voter).unwrap();
        client.try_join(&nominee).unwrap();

        client.try_vote_payout(&voter, &nominee, &0u32).unwrap();
        assert!(client.try_vote_payout(&voter, &nominee, &0u32).is_err());
    }

    #[test]
    fn test_report_late_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.contribution_deadline_seconds = 1u64; // Very short deadline
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let late_member = Address::generate(&env);
        let reporter = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&late_member).unwrap();
        client.try_join(&reporter).unwrap();

        // Manually simulate a late contribution by advancing ledger time
        // For this test, we just check the function doesn't error on non-existent contribution
        // A more complete test would require mocking the contribution as late
        env.ledger().with_mut(|l| {
            l.timestamp = 1000; // Far in future, after deadline
        });

        // Try to report as late (should fail since no late contribution recorded)
        // But let's first contribute late
        client.try_contribute(&late_member, &config.contribution_amount, &0u32).unwrap();

        // Now report should succeed — contribution was made at timestamp=1000,
        // well past the 1-second deadline, so on_time=false
        assert!(client.try_report_late(&reporter, &late_member, &0u32).is_ok());
    }

    #[test]
    fn test_raise_dispute_happy_path() {
        let env = Env::default();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();
        client.try_join(&member).unwrap();

        assert!(client.try_raise_dispute(&member, &evidence_hash).is_ok());
        assert_eq!(client.get_status().status, CircleStatus::Disputed);
    }

    #[test]
    fn test_raise_dispute_duplicate() {
        let env = Env::default();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();
        client.try_join(&member).unwrap();

        client.try_raise_dispute(&member, &evidence_hash).unwrap();
        assert!(client.try_raise_dispute(&member, &evidence_hash).is_err());
    }

    #[test]
    fn test_resolve_dispute_happy_path() {
        let env = Env::default();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();
        client.try_join(&member).unwrap();
        client.try_raise_dispute(&member, &evidence_hash).unwrap();

        assert!(client.try_resolve_dispute(&admin, &1u32).is_ok()); // RESOLVE_DISMISS = 1
        assert_eq!(client.get_status().status, CircleStatus::Active);
    }

    #[test]
    fn test_trigger_payout_random_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 0u32; // PAYOUT_RANDOM
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap();

        assert!(client.try_trigger_payout(&admin, &0u32).is_ok());
        assert_eq!(client.get_status().current_round, 1u32);
    }

    #[test]
    fn test_trigger_payout_fixed_happy_path() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.payout_type = 1u32; // PAYOUT_FIXED
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap();

        assert!(client.try_trigger_payout(&admin, &0u32).is_ok());
    }

    #[test]
    fn test_trigger_payout_unauthorized() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        let unauthorized = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap();

        assert!(client.try_trigger_payout(&unauthorized, &0u32).is_err());
    }

    #[test]
    fn test_pause_unpause_extended() {
        let env = Env::default();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let member = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();

        // Initially can join
        assert!(client.try_join(&member).is_ok());

        // Pause circle
        assert!(client.try_pause_circle(&admin).is_ok());

        // Cannot join while paused
        assert!(client.try_join(&other).is_err());

        // Cannot contribute while paused
        assert!(client.try_contribute(&member, &config.contribution_amount, &0u32).is_err());

        // Unpause
        assert!(client.try_unpause_circle(&admin).is_ok());

        // Can join again
        assert!(client.try_join(&other).is_ok());
    }

    #[test]
    fn test_pause_unpause_unauthorized() {
        let env = Env::default();
        let config = create_config(&env);
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);
        let unauthorized = Address::generate(&env);

        env.mock_all_auths();

        assert!(client.try_pause_circle(&unauthorized).is_err());
        assert!(client.try_unpause_circle(&unauthorized).is_err());
    }

    // ===== Issue 1: Allowlist Tests =====

    #[test]
    fn test_empty_allowlist_permits_all() {
        // No allowlist set — anyone can join (existing behaviour preserved)
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);
        let member = Address::generate(&env);

        env.mock_all_auths();
        // No allowlist configured, join should succeed
        assert!(client.try_join(&member).is_ok());
        assert_eq!(client.get_allowlist().len(), 0);
    }

    #[test]
    fn test_allowlist_permits_allowlisted_member() {
        let env = Env::default();
        let mut config = create_config(&env);
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);
        let allowed = Address::generate(&env);

        env.mock_all_auths();
        // Set allowlist to exactly [allowed]
        let mut allowlist = soroban_sdk::Vec::new(&env);
        allowlist.push_back(allowed.clone());
        client.set_allowlist(&admin, &allowlist);

        // Allowed address should join successfully
        assert!(client.try_join(&allowed).is_ok());
    }

    #[test]
    fn test_allowlist_blocks_non_allowlisted_member() {
        let env = Env::default();
        let mut config = create_config(&env);
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);
        let allowed = Address::generate(&env);
        let outsider = Address::generate(&env);

        env.mock_all_auths();
        // Set allowlist to exactly [allowed]
        let mut allowlist = soroban_sdk::Vec::new(&env);
        allowlist.push_back(allowed.clone());
        client.set_allowlist(&admin, &allowlist);

        // Outsider (not on allowlist) should be rejected
        let result = client.try_join(&outsider);
        assert!(result.is_err());
    }

    #[test]
    fn test_allowlist_only_admin_can_set() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);
        let non_admin = Address::generate(&env);

        env.mock_all_auths_allowing_non_root_auth();
        // Non-admin trying to set allowlist should fail
        let allowlist: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
        // We remove the auto-auth and verify it fails without admin creds
        let result = client.try_set_allowlist(&non_admin, &allowlist);
        assert!(result.is_err());
    }

    #[test]
    fn test_allowlist_get_returns_set_list() {
        let env = Env::default();
        let mut config = create_config(&env);
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);
        let a1 = Address::generate(&env);
        let a2 = Address::generate(&env);

        env.mock_all_auths();
        let mut allowlist = soroban_sdk::Vec::new(&env);
        allowlist.push_back(a1.clone());
        allowlist.push_back(a2.clone());
        client.set_allowlist(&admin, &allowlist);

        let stored = client.get_allowlist();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored.get(0).unwrap(), a1);
        assert_eq!(stored.get(1).unwrap(), a2);
    }

    // ===== Issue 4: Fee BPS Tests =====

    #[test]
    fn test_trigger_payout_zero_fee_by_default() {
        // When no fee_bps is set, net == pool and total_fees stays 0
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.total_rounds = 1u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();

        let amount = config.contribution_amount;
        mint_tokens(&env, &token, &m1, amount);
        mint_tokens(&env, &token, &m2, amount);
        client.try_contribute(&m1, &amount, &0u32).unwrap();
        client.try_contribute(&m2, &amount, &0u32).unwrap();
        client.try_trigger_payout(&admin, &0u32).unwrap();

        // No fee set — total_fees should be 0
        let circle = client.get_status();
        assert_eq!(circle.total_fees, 0i128);
    }

    #[test]
    fn test_trigger_payout_fee_collected_to_treasury() {
        // fee_bps = 50 (0.5%), treasury receives fee, winner receives net
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.total_rounds = 1u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();

        // Configure 0.5% fee and a treasury address
        let treasury = Address::generate(&env);
        client.set_fee_bps(&admin, &50u32);
        client.set_treasury(&admin, &treasury);

        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();

        let amount = config.contribution_amount; // 100_0000000
        mint_tokens(&env, &token, &m1, amount);
        mint_tokens(&env, &token, &m2, amount);
        client.try_contribute(&m1, &amount, &0u32).unwrap();
        client.try_contribute(&m2, &amount, &0u32).unwrap();
        client.try_trigger_payout(&admin, &0u32).unwrap();

        // pool = 200_0000000, fee = 0.5% = 1_0000000, net = 199_0000000
        let pool = amount * 2;
        let expected_fee = pool * 50 / 10_000; // = 1_0000000
        let expected_net = pool - expected_fee;

        let circle = client.get_status();
        assert_eq!(circle.total_fees, expected_fee);
        assert_eq!(circle.total_payouts, expected_net);

        // Treasury should have received the fee
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        let treasury_balance = token_client.balance(&treasury);
        assert_eq!(treasury_balance, expected_fee);
    }

    #[test]
    fn test_trigger_payout_fee_bps_max_boundary() {
        // fee_bps = 10000 (100%) is valid math — net = 0, fee = pool
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.total_rounds = 1u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();

        let treasury = Address::generate(&env);
        client.set_fee_bps(&admin, &10000u32);
        client.set_treasury(&admin, &treasury);

        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();

        let amount = config.contribution_amount;
        mint_tokens(&env, &token, &m1, amount);
        mint_tokens(&env, &token, &m2, amount);
        client.try_contribute(&m1, &amount, &0u32).unwrap();
        client.try_contribute(&m2, &amount, &0u32).unwrap();
        client.try_trigger_payout(&admin, &0u32).unwrap();

        let pool = amount * 2;
        let circle = client.get_status();
        assert_eq!(circle.total_fees, pool);
        assert_eq!(circle.total_payouts, 0i128);
    }

    #[test]
    fn test_set_fee_bps_only_admin_can_set() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);
        let non_admin = Address::generate(&env);

        env.mock_all_auths_allowing_non_root_auth();
        let result = client.try_set_fee_bps(&non_admin, &50u32);
        assert!(result.is_err());
    }
}
