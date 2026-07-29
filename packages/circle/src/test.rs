#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests {
    use soroban_sdk::{Address, BytesN, Env, String};
    use soroban_sdk::testutils::Address as _;
    use crate as circle;
    use circle::CircleError;

    const MEMBER_ACTIVE: u32 = 0u32;

    fn create_config(env: &Env) -> circle::types::CircleConfig {
        circle::types::CircleConfig {
            organizer: Address::generate(env),
            token: Address::generate(env), // overridden by helper
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
        }
    }

    fn setup_test_env<'a>(env: &Env, config: &mut circle::types::CircleConfig) -> (Address, circle::CircleClient<'a>) {
        let token_admin = Address::generate(env);
        let token = env.register_stellar_asset_contract(token_admin);
        config.token = token.clone();

        let admin = config.organizer.clone();
        let factory = Address::generate(env);
        let contract_id = env.register(circle::Circle, (&admin, &factory, &*config));
        let client = circle::CircleClient::new(env, &contract_id);
        (token, client)
    }

    fn mint_tokens(env: &Env, token: &Address, recipient: &Address, amount: i128) {
        let token_client = soroban_sdk::token::StellarAssetClient::new(env, token);
        token_client.mint(recipient, &amount);
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);
        let status = client.get_status();
        assert_eq!(status.status, 0u32);
    }

    #[test]
    fn test_join() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);
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
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        client.try_join(&Address::generate(&env)).unwrap();
        client.try_join(&Address::generate(&env)).unwrap();
        assert!(client.try_join(&Address::generate(&env)).is_err());
    }

    #[test]
    fn test_duplicate_join() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);
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
        let (token, client) = setup_test_env(&env, &mut config);
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();

        mint_tokens(&env, &token, &m1, config.contribution_amount);
        assert!(client.try_contribute(&m1, &config.contribution_amount, &0u32).is_ok());
    }

    #[test]
    fn test_contribute_wrong_amount() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let (token, client) = setup_test_env(&env, &mut config);
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();

        mint_tokens(&env, &token, &m1, 50_0000000i128);
        assert!(client.try_contribute(&m1, &50_0000000i128, &0u32).is_err());
    }

    #[test]
    fn test_contribute_not_member() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);
        let outsider = Address::generate(&env);

        env.mock_all_auths();
        assert!(client.try_contribute(&outsider, &config.contribution_amount, &0u32).is_err());
    }

    #[test]
    fn test_exit() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 2u32;
        let (token, client) = setup_test_env(&env, &mut config);
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();

        mint_tokens(&env, &token, &m1, config.contribution_amount);
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap();
        assert!(client.try_exit_circle(&m1).is_ok());
    }

    #[test]
    fn test_pause_unpause() {
        let env = Env::default();
        let mut config = create_config(&env);
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);

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
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);
        let member = Address::generate(&env);

        // No env.mock_all_auths() — should fail authorization
        assert!(client.try_join(&member).is_err());
    }

    #[test]
    fn test_full_lifecycle() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 3u32;
        config.total_rounds = 3u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);

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
        mint_tokens(&env, &token, &m1, config.contribution_amount);
        mint_tokens(&env, &token, &m2, config.contribution_amount);
        mint_tokens(&env, &token, &m3, config.contribution_amount);
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap();
        client.try_contribute(&m3, &config.contribution_amount, &0u32).unwrap();
        client.try_trigger_payout(&admin, &0u32).unwrap();
        assert_eq!(client.get_status().current_round, 1u32);

        // Round 1
        mint_tokens(&env, &token, &m1, config.contribution_amount);
        mint_tokens(&env, &token, &m2, config.contribution_amount);
        mint_tokens(&env, &token, &m3, config.contribution_amount);
        client.try_contribute(&m1, &config.contribution_amount, &1u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &1u32).unwrap();
        client.try_contribute(&m3, &config.contribution_amount, &1u32).unwrap();
        client.try_trigger_payout(&admin, &1u32).unwrap();

        // Round 2
        mint_tokens(&env, &token, &m1, config.contribution_amount);
        mint_tokens(&env, &token, &m2, config.contribution_amount);
        mint_tokens(&env, &token, &m3, config.contribution_amount);
        client.try_contribute(&m1, &config.contribution_amount, &2u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &2u32).unwrap();
        client.try_contribute(&m3, &config.contribution_amount, &2u32).unwrap();
        client.try_trigger_payout(&admin, &2u32).unwrap();

        // Should be completed
        assert_eq!(client.get_status().status, 2u32);
    }
}
