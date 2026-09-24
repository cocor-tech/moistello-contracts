#![cfg(test)]

//! Storage-migration coverage for the contribution replay guard (#326).
//!
//! Circles that were deployed before the `DataKey::ContributionExists` uniqueness entry
//! existed keep their history in `DataKey::Contributions` only. After an upgrade to the
//! version that adds the entry, an already recorded `(member, round)` must still be
//! rejected, the entry must be backfilled so later checks are O(1), and no recorded
//! history may be lost.

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

use crate::types::{CircleConfig, DataKey, PAYOUT_FIXED};
use crate::{Circle, CircleArgs, CircleClient, CircleError};

const AMOUNT: i128 = 100;
const FUNDING: i128 = 1_000;

fn create_config(env: &Env, token: &Address) -> CircleConfig {
    CircleConfig {
        organizer: Address::generate(env),
        token: token.clone(),
        name: String::from_str(env, "Migration Circle"),
        contribution_amount: AMOUNT,
        max_members: 2,
        payout_type: PAYOUT_FIXED,
        total_rounds: 3,
        contribution_deadline_seconds: 604_800,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 86_400,
        max_strikes: 3,
        slug: String::from_str(env, "migration-circle"),
    }
}

/// Deploys an ACTIVE two-member circle with fully funded members.
fn setup_active_circle<'a>(env: &'a Env) -> (CircleClient<'a>, Address, Vec<Address>) {
    env.mock_all_auths();

    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let config = create_config(env, &token);
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(env, &contract_id);

    let token_client = soroban_sdk::token::StellarAssetClient::new(env, &token);
    let mut members = Vec::new(env);
    for _ in 0..2 {
        let member = Address::generate(env);
        token_client.mint(&member, &FUNDING);
        client.join(&member);
        members.push_back(member);
    }

    (client, contract_id, members)
}

fn uniqueness_entry(
    env: &Env,
    contract_id: &Address,
    member: &Address,
    round: u32,
) -> Option<bool> {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::ContributionExists(member.clone(), round))
    })
}

#[test]
fn test_legacy_contribution_still_blocks_replay_after_upgrade() {
    let env = Env::default();
    let (client, contract_id, members) = setup_active_circle(&env);
    let first = members.get(0).unwrap();
    let second = members.get(1).unwrap();

    client.contribute(&first, &AMOUNT, &0u32);

    // Simulate the state written by the previous contract version: the contribution is
    // recorded in the history, without a uniqueness entry.
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::ContributionExists(first.clone(), 0u32));
    });
    assert_eq!(uniqueness_entry(&env, &contract_id, &first, 0), None);

    // The upgrade must not make an already recorded contribution replayable: with no
    // uniqueness entry the guard falls back to the recorded history and rejects the replay.
    assert_eq!(
        client.try_contribute(&first, &AMOUNT, &0u32),
        Err(Ok(CircleError::AlreadyContributed))
    );

    // A rejected invocation is reverted as a whole, so the guard leaves storage untouched:
    // no half-written uniqueness entry and no extra contribution record.
    assert_eq!(uniqueness_entry(&env, &contract_id, &first, 0), None);
    let history = client.get_contributions(&first, &0, &10);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().round, 0u32);
    assert_eq!(history.get(0).unwrap().amount, AMOUNT);

    // A member with no pre-upgrade record goes through the new path: recording the
    // contribution writes its uniqueness entry, and the following replay is answered by it.
    client.contribute(&second, &AMOUNT, &0u32);
    assert_eq!(uniqueness_entry(&env, &contract_id, &second, 0), Some(true));
    assert_eq!(
        client.try_contribute(&second, &AMOUNT, &0u32),
        Err(Ok(CircleError::AlreadyContributed))
    );
    assert_eq!(client.get_contributions(&second, &0, &10).len(), 1);
}
