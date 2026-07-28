#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests {
    use soroban_sdk::{Address, Env, String};
    use soroban_sdk::testutils::Address as _;
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
