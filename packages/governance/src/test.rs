#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests {
    use crate as governance;
    use governance::types::{
        GovernanceConfig, GovernanceError, Proposal, ProposalStatus, VoteType,
    };
    use governance::Governance;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

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

        env.ledger()
            .set_timestamp(env.ledger().timestamp() + CONFIG_TIMELOCK_SECONDS - 1);
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

        env.ledger()
            .set_timestamp(env.ledger().timestamp() + CONFIG_TIMELOCK_SECONDS);
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

        env.ledger()
            .set_timestamp(env.ledger().timestamp() + CONFIG_TIMELOCK_SECONDS);
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

        env.ledger()
            .set_timestamp(env.ledger().timestamp() + config.voting_period_seconds + 1);
        client.finalize_proposal(&id);
        let proposal = client.get_proposal(&id);
        assert_eq!(proposal.status, governance::types::ProposalStatus::Queued);

        // Proposal-execution timelock (config.timelock_seconds) is distinct
        // from and unaffected by the new, fixed config-update timelock.
        let too_early = client.try_execute_proposal(&id);
        assert_eq!(too_early, Err(Ok(GovernanceError::TimelockNotElapsed)));
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

    // ── Self-delegation guard tests ───────────────────────────────────────────

    #[test]
    fn test_self_delegation_is_rejected() {
        // A user must never be able to delegate to themselves — the guard must
        // fire before any storage write occurs.
        let env = Env::default();
        let (client, _) = setup(&env);
        let alice = Address::generate(&env);

        let result = client.try_delegate(&alice, &alice);
        assert_eq!(result, Err(Ok(GovernanceError::CircularDelegation)));
    }

    #[test]
    fn test_direct_circular_delegation_a_to_b_then_b_to_a_rejected() {
        // A→B is allowed; B→A must be rejected (would form a cycle).
        let env = Env::default();
        let (client, _) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.delegate(&alice, &bob);

        // Bob already has alice as a delegator, so he cannot delegate outward.
        let result = client.try_delegate(&bob, &alice);
        assert_eq!(result, Err(Ok(GovernanceError::CircularDelegation)));
    }

    #[test]
    fn test_three_hop_delegation_loop_rejected() {
        // A→B, B attempts→C: rejected because B already has a delegator (A).
        // C→A: rejected because A is already being delegated to by none,
        //      but A has already delegated away; A→B means A has delegation set,
        //      so C→A is fine IF A's delegators list is empty.  The real guard
        //      is that B cannot delegate further (B has delegators).
        let env = Env::default();
        let (client, _) = setup(&env);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);

        client.delegate(&a, &b);

        // B has A as a delegator → B cannot delegate to anyone.
        assert_eq!(client.try_delegate(&b, &c), Err(Ok(GovernanceError::CircularDelegation)));
    }

    #[test]
    fn test_re_delegation_replaces_previous_delegatee() {
        // Delegating to a second target after already having one should
        // cleanly replace the first: the old delegatee must no longer carry
        // the delegator in their list, and the new delegatee must gain it.
        let env = Env::default();
        let (client, admin) = setup(&env);
        let delegator = Address::generate(&env);
        let first = Address::generate(&env);
        let second = Address::generate(&env);

        client.delegate(&delegator, &first);
        // Redirect to a different delegatee.
        client.delegate(&delegator, &second);

        // Vote via `second` — delegator's power must now flow to second, not first.
        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[11u8; 32]);
        let id = client.create_proposal(
            &admin,
            &create_config().proposal_deposit,
            &action,
            &description,
        );

        client.cast_vote(&second, &id, &VoteType::For);
        // second (1) + delegator (1) = 2 votes
        assert_eq!(client.get_proposal(&id).votes_for, 2);

        // `first` votes independently — their own vote only (1).
        client.cast_vote(&first, &id, &VoteType::For);
        assert_eq!(client.get_proposal(&id).votes_for, 3);
    }

    #[test]
    fn test_revoke_delegation_allows_independent_voting() {
        // After revoking, the former delegator must be able to cast their own
        // vote, and the former delegatee must not carry their power.
        let env = Env::default();
        let (client, admin) = setup(&env);
        let delegator = Address::generate(&env);
        let delegatee = Address::generate(&env);

        client.delegate(&delegator, &delegatee);
        client.revoke_delegation(&delegator);

        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[12u8; 32]);
        let id = client.create_proposal(
            &admin,
            &create_config().proposal_deposit,
            &action,
            &description,
        );

        // delegatee votes for themselves only (1 vote).
        client.cast_vote(&delegatee, &id, &VoteType::For);
        assert_eq!(client.get_proposal(&id).votes_for, 1);

        // Former delegator can now vote independently.
        client.cast_vote(&delegator, &id, &VoteType::Against);
        assert_eq!(client.get_proposal(&id).votes_against, 1);
    }

    #[test]
    fn test_delegator_cannot_vote_directly_while_delegated() {
        // A delegator who has an active delegation must not be able to cast a
        // direct vote — their power is exercised only through the delegatee.
        let env = Env::default();
        let (client, admin) = setup(&env);
        let delegator = Address::generate(&env);
        let delegatee = Address::generate(&env);

        client.delegate(&delegator, &delegatee);

        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[13u8; 32]);
        let id = client.create_proposal(
            &admin,
            &create_config().proposal_deposit,
            &action,
            &description,
        );

        assert_eq!(
            client.try_cast_vote(&delegator, &id, &VoteType::For),
            Err(Ok(GovernanceError::Unauthorized))
        );
    }

    #[test]
    fn test_delegate_and_revoke_idempotent_on_no_prior_delegation() {
        // Revoking when no delegation exists must not panic — it is a no-op.
        let env = Env::default();
        let (client, _) = setup(&env);
        let alice = Address::generate(&env);

        // Should succeed silently with no prior delegation set.
        client.revoke_delegation(&alice);
    }

    #[test]
    fn test_delegation_blocked_when_contract_paused() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        client.pause(&admin);

        let delegator = Address::generate(&env);
        let delegatee = Address::generate(&env);

        let result = client.try_delegate(&delegator, &delegatee);
        assert_eq!(result, Err(Ok(GovernanceError::ContractPaused)));
    }

    #[test]
    fn test_delegation_and_vote_weight_accumulates_for_multiple_delegators() {
        // Two distinct addresses each delegate to the same delegatee; when
        // the delegatee votes, the tally should include all three votes (their
        // own + the two delegators').
        let env = Env::default();
        let (client, admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let delegatee = Address::generate(&env);

        client.delegate(&alice, &delegatee);
        client.delegate(&bob, &delegatee);

        let action = governance::types::ProposalAction {
            target_contract: admin.clone(),
            method: Symbol::new(&env, "noop"),
            args: Vec::new(&env),
        };
        let description = BytesN::from_array(&env, &[14u8; 32]);
        let id = client.create_proposal(
            &admin,
            &create_config().proposal_deposit,
            &action,
            &description,
        );

        client.cast_vote(&delegatee, &id, &VoteType::For);
        // delegatee (1) + alice (1) + bob (1) = 3
        assert_eq!(client.get_proposal(&id).votes_for, 3);
    }
}
