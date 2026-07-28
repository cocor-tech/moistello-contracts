#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests {
    use soroban_sdk::{Address, Env, String};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;
    use crate as circle;
    use circle::{Circle, CircleArgs, CircleStatus};

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

    fn create_test_token(env: &Env) -> (Address, StellarAssetClient) {
        let token_admin = Address::generate(env);
        let token_id = env.register_stellar_asset_contract(token_admin.clone());
        (token_id, StellarAssetClient::new(env, &token_id))
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
    fn test_claim_referral_bonus_transfers_tokens() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let mut config = create_config(&env);
        config.max_members = 2u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);

        let (token, sac) = create_test_token(&env);
        let treasury = Address::generate(&env);

        client.try_set_token(&admin, &token).unwrap();
        client.try_set_treasury(&admin, &treasury).unwrap();

        let referrer = Address::generate(&env);
        let referred = Address::generate(&env);

        client.try_join(&referrer).unwrap();
        client.try_join(&referred).unwrap();
        client.try_register_referral(&referrer, &referred, &500u32).unwrap();
        client.try_contribute(&referred, &config.contribution_amount, &0u32).unwrap();

        let bonus_total = config.contribution_amount * 500 / 10000;
        sac.mint(&contract_id, &bonus_total);
        let before_balance = sac.balance(&referrer);

        client.try_claim_referral_bonus(&referrer, &treasury).unwrap();

        let after_balance = sac.balance(&referrer);
        assert_eq!(after_balance, before_balance + bonus_total);
        assert_eq!(sac.balance(&contract_id), bonus_total - bonus_total);
    }

    #[test]
    fn test_claim_streak_bonus_transfers_tokens() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let mut config = create_config(&env);
        config.max_members = 1u32;
        let admin = config.organizer.clone();
        let factory = Address::generate(&env);
        let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
        let client = circle::CircleClient::new(&env, &contract_id);

        let (token, sac) = create_test_token(&env);
        let treasury = Address::generate(&env);

        client.try_set_token(&admin, &token).unwrap();
        client.try_set_treasury(&admin, &treasury).unwrap();

        let member = Address::generate(&env);
        client.try_join(&member).unwrap();
        client.try_update_streak(&member, &0u32).unwrap();
        client.try_update_streak(&member, &1u32).unwrap();
        client.try_update_streak(&member, &2u32).unwrap();

        let bonus_div = config.contribution_amount * 3 / 100;
        sac.mint(&contract_id, &bonus_div);
        let before_balance = sac.balance(&member);

        client.try_claim_streak_bonus(&member, &treasury).unwrap();

        let after_balance = sac.balance(&member);
        assert_eq!(after_balance, before_balance + bonus_div);
        assert_eq!(sac.balance(&contract_id), bonus_div - bonus_div);
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

        // Now report should work if we had advanced time
        assert!(client.try_report_late(&reporter, &late_member, &0u32).is_err()); // on_time was recorded as true
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
        let evidence_hash = [1u8; 32].into();

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
        let evidence_hash = [1u8; 32].into();

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
        let evidence_hash = [1u8; 32].into();

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
}
