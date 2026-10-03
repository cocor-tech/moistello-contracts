#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests {
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};
    use crate as governance;
    use governance::Governance;
    use governance::types::{GovernanceConfig, GovernanceError};

    const CONFIG_TIMELOCK_SECONDS: u64 = 172_800; // 48 hours

    fn create_config() -> GovernanceConfig {
        GovernanceConfig {
            proposal_deposit: 100i128,
            voting_period_seconds: 604800u64,
            timelock_seconds: 86400u64,
            quorum_votes: 1u32,
            pass_threshold_bps: 5000u32,
            min_proposal_deposit: 100i128,
        }
    }

    fn setup(env: &Env) -> (governance::GovernanceClient<'_>, Address) {
        let admin = Address::generate(env);
        let config = create_config();
        let contract_id = env.register(Governance, ());
        let client = governance::GovernanceClient::new(env, &contract_id);
        env.mock_all_auths();
        client.init(&admin, &config);
        (client, admin)
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        let (client, _admin) = setup(&env);
        let config = client.get_config();
        assert_eq!(config.quorum_votes, 1u32);
    }

    #[test]
    fn test_queue_config_update_sets_48h_timelock() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let mut new_config = create_config();
        new_config.quorum_votes = 5u32;

        let now = env.ledger().timestamp();
        client.queue_config_update(&admin, &new_config);

        let pending = client.get_pending_config_update().unwrap();
        assert_eq!(pending.queued_at, now);
        assert_eq!(pending.executable_at, now + CONFIG_TIMELOCK_SECONDS);
        // Not applied yet — old config still active.
        assert_eq!(client.get_config().quorum_votes, 1u32);
    }

    #[test]
    fn test_execute_config_update_before_timelock_fails() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let mut new_config = create_config();
        new_config.quorum_votes = 5u32;
        client.queue_config_update(&admin, &new_config);

        env.ledger().set_timestamp(env.ledger().timestamp() + CONFIG_TIMELOCK_SECONDS - 1);
        let result = client.try_execute_config_update();
        assert_eq!(result, Err(Ok(GovernanceError::TimelockNotElapsed)));
        assert_eq!(client.get_config().quorum_votes, 1u32);
    }

    #[test]
    fn test_execute_config_update_after_timelock_applies() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let mut new_config = create_config();
        new_config.quorum_votes = 5u32;
        client.queue_config_update(&admin, &new_config);

        env.ledger().set_timestamp(env.ledger().timestamp() + CONFIG_TIMELOCK_SECONDS);
        client.execute_config_update();

        assert_eq!(client.get_config().quorum_votes, 5u32);
        assert!(client.get_pending_config_update().is_none());
    }

    #[test]
    fn test_cancel_config_update_during_timelock() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let mut new_config = create_config();
        new_config.quorum_votes = 5u32;
        client.queue_config_update(&admin, &new_config);

        client.cancel_config_update(&admin);
        assert!(client.get_pending_config_update().is_none());

        env.ledger().set_timestamp(env.ledger().timestamp() + CONFIG_TIMELOCK_SECONDS);
        let result = client.try_execute_config_update();
        assert_eq!(result, Err(Ok(GovernanceError::NoPendingConfigUpdate)));
        assert_eq!(client.get_config().quorum_votes, 1u32);
    }

    #[test]
    fn test_cannot_queue_second_update_while_one_pending() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let mut new_config = create_config();
        new_config.quorum_votes = 5u32;
        client.queue_config_update(&admin, &new_config);

        let result = client.try_queue_config_update(&admin, &new_config);
        assert_eq!(result, Err(Ok(GovernanceError::ConfigUpdateAlreadyQueued)));
    }

    #[test]
    fn test_queue_config_update_unauthorized() {
        let env = Env::default();
        let (client, _admin) = setup(&env);
        let not_admin = Address::generate(&env);
        let new_config = create_config();

        let result = client.try_queue_config_update(&not_admin, &new_config);
        assert_eq!(result, Err(Ok(GovernanceError::Unauthorized)));
    }

    #[test]
    fn test_queue_config_update_invalid_config_rejected() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let mut bad_config = create_config();
        bad_config.pass_threshold_bps = 0u32;

        let result = client.try_queue_config_update(&admin, &bad_config);
        assert_eq!(result, Err(Ok(GovernanceError::InvalidConfig)));
        assert!(client.get_pending_config_update().is_none());
    }

    #[test]
    fn test_proposal_lifecycle_still_uses_its_own_timelock() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let config = create_config();

        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[0u8; 32]);
        let id = client.create_proposal(&admin, &config.proposal_deposit, &action, &description);

        let voter = Address::generate(&env);
        client.cast_vote(&voter, &id, &governance::types::VoteType::For);

        env.ledger().set_timestamp(env.ledger().timestamp() + config.voting_period_seconds + 1);
        client.finalize_proposal(&id);
        let proposal = client.get_proposal(&id);
        assert_eq!(proposal.status, governance::types::ProposalStatus::Queued);

        // Proposal-execution timelock (config.timelock_seconds) is distinct
        // from and unaffected by the new, fixed config-update timelock.
        let too_early = client.try_execute_proposal(&id);
        assert_eq!(too_early, Err(Ok(GovernanceError::TimelockNotElapsed)));
    }

    #[test]
    fn test_proposal_deposit_refund_on_pass() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let config = create_config();

        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[1u8; 32]);
        let id = client.create_proposal(&admin, &config.proposal_deposit, &action, &description);
        assert_eq!(client.get_deposit(&id), Some(config.proposal_deposit));

        let voter = Address::generate(&env);
        client.cast_vote(&voter, &id, &governance::types::VoteType::For);

        env.ledger().set_timestamp(env.ledger().timestamp() + config.voting_period_seconds + 1);
        client.finalize_proposal(&id);

        let proposal = client.get_proposal(&id);
        assert_eq!(proposal.status, governance::types::ProposalStatus::Queued);
        assert_eq!(client.get_deposit(&id), None);
    }

    #[test]
    fn test_proposal_deposit_forfeit_on_defeat() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let config = create_config();

        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[2u8; 32]);
        let id = client.create_proposal(&admin, &config.proposal_deposit, &action, &description);
        assert_eq!(client.get_deposit(&id), Some(config.proposal_deposit));

        let voter = Address::generate(&env);
        client.cast_vote(&voter, &id, &governance::types::VoteType::Against);

        env.ledger().set_timestamp(env.ledger().timestamp() + config.voting_period_seconds + 1);
        client.finalize_proposal(&id);

        let proposal = client.get_proposal(&id);
        assert_eq!(proposal.status, governance::types::ProposalStatus::Defeated);
        assert_eq!(client.get_deposit(&id), None);
    }

    #[test]
    fn test_proposal_spam_insufficient_deposit_rejected() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let config = create_config();

        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[3u8; 32]);

        // Spam scenario 1: 0 deposit
        let zero_res = client.try_create_proposal(&admin, &0i128, &action, &description);
        assert_eq!(zero_res, Err(Ok(GovernanceError::InsufficientDeposit)));

        // Spam scenario 2: below min_proposal_deposit
        let below_min = config.min_proposal_deposit - 1;
        let below_res = client.try_create_proposal(&admin, &below_min, &action, &description);
        assert_eq!(below_res, Err(Ok(GovernanceError::InsufficientDeposit)));

        // Spam scenario 3: negative deposit
        let neg_res = client.try_create_proposal(&admin, &-10i128, &action, &description);
        assert_eq!(neg_res, Err(Ok(GovernanceError::InsufficientDeposit)));
    }

    #[test]
    fn test_proposal_deposit_refund_on_cancel() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let config = create_config();

        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[4u8; 32]);
        let id = client.create_proposal(&admin, &config.proposal_deposit, &action, &description);
        assert_eq!(client.get_deposit(&id), Some(config.proposal_deposit));

        client.cancel_proposal(&admin, &id);
        let proposal = client.get_proposal(&id);
        assert_eq!(proposal.status, governance::types::ProposalStatus::Cancelled);
        assert_eq!(client.get_deposit(&id), None);
    }
    #[test]
    fn test_abstentions_count_toward_quorum_but_not_support_ratio() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let mut config = create_config();
        config.quorum_votes = 2;
        client.queue_config_update(&admin, &config);
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + CONFIG_TIMELOCK_SECONDS);
        client.execute_config_update();

        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[1u8; 32]);
        let id = client.create_proposal(&admin, &config.proposal_deposit, &action, &description);

        client.cast_vote(&Address::generate(&env), &id, &VoteType::For);
        client.cast_vote(&Address::generate(&env), &id, &VoteType::Abstain);

        env.ledger()
            .set_timestamp(env.ledger().timestamp() + config.voting_period_seconds + 1);
        client.finalize_proposal(&id);

        assert_eq!(client.get_proposal(&id).status, ProposalStatus::Queued);
    }

    #[test]
    fn test_tally_arithmetic_handles_adversarial_vote_weights() {
        let env = Env::default();
        let config = GovernanceConfig {
            quorum_votes: 1,
            pass_threshold_bps: 5000,
            ..create_config()
        };
        let action = governance::types::ProposalAction {
            target_contract: Address::generate(&env),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let base = Proposal {
            id: 0,
            proposer: Address::generate(&env),
            deposit_amount: 100,
            action,
            description: BytesN::from_array(&env, &[2u8; 32]),
            status: ProposalStatus::Active,
            created_at: 0,
            voting_ends_at: 0,
            timelock_ends_at: 0,
            votes_for: 0,
            votes_against: 0,
            votes_abstain: 0,
        };

        let abstain_only = Proposal {
            votes_abstain: 10,
            ..base.clone()
        };
        assert_eq!(governance::contract::proposal_passes(&abstain_only, &config), Ok(false));

        let overflowing_participation = Proposal {
            votes_for: i128::MAX,
            votes_abstain: 1,
            ..base.clone()
        };
        assert_eq!(
            governance::contract::proposal_passes(&overflowing_participation, &config),
            Err(GovernanceError::InvalidConfig)
        );

        let overflowing_support_scale = Proposal {
            votes_for: (i128::MAX / 10_000) + 1,
            votes_against: 1,
            ..base
        };
        assert_eq!(
            governance::contract::proposal_passes(&overflowing_support_scale, &config),
            Err(GovernanceError::InvalidConfig)
        );
    }

    #[test]
    fn test_delegation_applies_at_vote_time_and_rejects_chains() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[9u8; 32]);
        let id = client.create_proposal(
            &admin,
            &create_config().proposal_deposit,
            &action,
            &description,
        );

        let delegator = Address::generate(&env);
        let delegatee = Address::generate(&env);
        let third = Address::generate(&env);

        assert_eq!(
            client.try_delegate(&delegator, &delegator),
            Err(Ok(GovernanceError::CircularDelegation))
        );
        client.delegate(&delegator, &delegatee);
        assert_eq!(
            client.try_delegate(&delegatee, &third),
            Err(Ok(GovernanceError::CircularDelegation))
        );
        assert_eq!(
            client.try_delegate(&third, &delegator),
            Err(Ok(GovernanceError::CircularDelegation))
        );
        assert_eq!(
            client.try_cast_vote(&delegator, &id, &VoteType::For),
            Err(Ok(GovernanceError::Unauthorized))
        );

        client.cast_vote(&delegatee, &id, &VoteType::For);
        let proposal = client.get_proposal(&id);
        assert_eq!(proposal.votes_for, 2);

        client.revoke_delegation(&delegator);
        let id2 = client.create_proposal(
            &admin,
            &create_config().proposal_deposit,
            &action,
            &description,
        );
        client.cast_vote(&delegatee, &id2, &VoteType::For);
        assert_eq!(client.get_proposal(&id2).votes_for, 1);
        client.cast_vote(&delegator, &id2, &VoteType::Against);
        assert_eq!(client.get_proposal(&id2).votes_against, 1);
    }

    #[test]
    fn test_proposal_metadata_page_paginates_without_status_filter() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[3u8; 32]);
        let id = client.create_proposal(
            &admin,
            &create_config().proposal_deposit,
            &action,
            &description,
        );

        // Attempt to expire while active and before voting period + execution window ends
        let result = client.try_expire_proposal(&id);
        assert_eq!(result, Err(Ok(GovernanceError::ProposalNotExpired)));
    }

    #[test]
    fn test_expire_proposal_past_deadline_succeeds_with_refund() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let config = create_config();
        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[4u8; 32]);
        let id = client.create_proposal(&admin, &config.proposal_deposit, &action, &description);

        // Advance timestamp past voting_ends_at + PROPOSAL_EXECUTION_WINDOW (7 days = 604,800s)
        let execution_window = 604_800u64;
        let expire_time =
            env.ledger().timestamp() + config.voting_period_seconds + execution_window + 1;
        env.ledger().set_timestamp(expire_time);

        client.expire_proposal(&id);

        let proposal = client.get_proposal(&id);
        assert_eq!(proposal.status, ProposalStatus::Expired);

        // Expired proposals can be queried by status
        let expired_list = client.get_proposals(&ProposalStatus::Expired, &10);
        assert_eq!(expired_list.len(), 1);
        assert_eq!(expired_list.get(0).unwrap().id, id);

        let first_description = BytesN::from_array(&env, &[1u8; 32]);
        let second_description = BytesN::from_array(&env, &[2u8; 32]);
        let third_description = BytesN::from_array(&env, &[3u8; 32]);
        client.create_proposal(
            &admin,
            &create_config().proposal_deposit,
            &action,
            &first_description,
        );
        client.create_proposal(
            &admin,
            &create_config().proposal_deposit,
            &action,
            &second_description,
        );
        client.create_proposal(
            &admin,
            &create_config().proposal_deposit,
            &action,
            &third_description,
        );

        let first_page = client.get_proposal_metadata_page(&0u64, &2u32);
        // NOTE (fix/compile): the listing includes the expired proposal, so
        // all 4 proposals are visible: page 1 = [expired id 0, id 1].
        assert_eq!(first_page.total, 4);
        assert_eq!(first_page.next_cursor, 2);
        assert_eq!(first_page.entries.len(), 2);
        assert_eq!(first_page.entries.get(0).unwrap().id, 0);
        assert_eq!(first_page.entries.get(1).unwrap().description, first_description);

        let second_page = client.get_proposal_metadata_page(&first_page.next_cursor, &50u32);
        assert_eq!(second_page.entries.len(), 2);
        assert_eq!(second_page.entries.get(0).unwrap().id, 2);
        assert_eq!(second_page.entries.get(0).unwrap().description, second_description);
    }
}
