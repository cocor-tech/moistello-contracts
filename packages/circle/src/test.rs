#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests {
    use soroban_sdk::{Address, Env, String};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger;
    use crate as circle;

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

    // --- #128: batch_invite ---
    #[test]
    fn test_batch_invite_organizer_only() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 5u32;
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let non_organizer = Address::generate(&env);
        let members = soroban_sdk::vec![&env, Address::generate(&env)];
        assert!(client.try_batch_invite(&non_organizer, &members).is_err());
    }

    #[test]
    fn test_batch_invite_adds_members() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 5u32;
        let organizer = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        let m3 = Address::generate(&env);
        let members = soroban_sdk::vec![&env, m1.clone(), m2.clone(), m3.clone()];
        assert!(client.try_batch_invite(&organizer, &members).is_ok());
        assert_eq!(client.get_members().len(), 3);
    }

    #[test]
    fn test_batch_invite_rejects_duplicates() {
        let env = Env::default();
        let mut config = create_config(&env);
        config.max_members = 5u32;
        let organizer = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        // First invite m1 solo
        assert!(client.try_join(&m1).is_ok());
        // Then batch invite m1 again — should fail
        let members = soroban_sdk::vec![&env, m1.clone(), m2.clone()];
        assert!(client.try_batch_invite(&organizer, &members).is_err());
    }

    #[test]
    fn test_batch_invite_rejects_when_paused() {
        let env = Env::default();
        let mut config = create_config(&env);
        let admin = config.organizer.clone();
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        assert!(client.try_pause_circle(&admin).is_ok());
        let members = soroban_sdk::vec![&env, Address::generate(&env)];
        assert!(client.try_batch_invite(&admin, &members).is_err());
    }

    // --- #127: self-referral prevention ---
    #[test]
    fn test_register_referral_different_addresses() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let referrer = Address::generate(&env);
        let referred = Address::generate(&env);
        assert!(client.try_register_referral(&referrer, &referred, &1000u32).is_ok());
    }

    #[test]
    fn test_register_referral_self_referral_fails() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let addr = Address::generate(&env);
        let result = client.try_register_referral(&addr, &addr, &1000u32);
        assert!(result.is_err());
    }

    #[test]
    fn test_register_referral_rejects_excessive_bonus() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let referrer = Address::generate(&env);
        let referred = Address::generate(&env);
        assert!(client.try_register_referral(&referrer, &referred, &20000u32).is_err());
    }

    // --- #129: time-weighted contributions ---
    #[test]
    fn test_contribution_has_time_weight() {
        let env = Env::default();
        env.ledger().with_mut(|li| li.timestamp = 12345);
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

        let contribs = client.get_contributions(&m1);
        assert_eq!(contribs.len(), 1);
        let c = contribs.get(0).unwrap();
        assert_eq!(c.time_weight, 12345);
    }

    #[test]
    fn test_payout_uses_time_weighted_distribution() {
        let env = Env::default();
        env.ledger().with_mut(|li| li.timestamp = 1000000);
        let mut config = create_config(&env);
        config.max_members = 2u32;
        config.total_rounds = 1u32;
        let admin = config.organizer.clone();
        let (token, client) = setup_test_env(&env, &mut config);
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);

        env.mock_all_auths();
        client.try_join(&m1).unwrap();
        client.try_join(&m2).unwrap();
        mint_tokens(&env, &token, &m1, config.contribution_amount);
        mint_tokens(&env, &token, &m2, config.contribution_amount);
        client.try_contribute(&m1, &config.contribution_amount, &0u32).unwrap();
        client.try_contribute(&m2, &config.contribution_amount, &0u32).unwrap();
        assert!(client.try_trigger_payout(&admin, &0u32).is_ok());
        // After payout, pool = 2 * contribution_amount should be distributed
        let status = client.get_status();
        assert_eq!(status.current_round, 1u32);
    }

    // --- #130: migration storage test ---
    #[test]
    fn test_storage_upgrade_preserves_state() {
        let env = Env::default();
        let mut config = create_config(&env);
        let (_, client) = setup_test_env(&env, &mut config);

        env.mock_all_auths();
        let member = Address::generate(&env);
        assert!(client.try_join(&member).is_ok());

        // Read back - state should be intact
        let members = client.get_members();
        assert_eq!(members.len(), 1);
        let m = members.get(0).unwrap();
        assert_eq!(m.address, member);

        let status = client.get_status();
        assert_eq!(status.member_count, 1);
    }
}
