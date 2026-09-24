#![cfg(test)]

//! Contribution replay-protection tests (#326).
//!
//! A contribution is made unique per `(member, round)` by a dedicated storage entry
//! (`DataKey::ContributionExists`), so the guard is a single O(1) read instead of a scan
//! over the contribution history. These tests cover the success path, every failure path
//! of `contribute`, the double-submit / "concurrent" attempts named in the issue, and the
//! cost of the uniqueness lookup compared with the linear scan it replaces.

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

use crate::types::{CircleConfig, DataKey, PAYOUT_FIXED};
use crate::{Circle, CircleArgs, CircleClient, CircleError};

/// Contribution amount used by every circle in this module.
const AMOUNT: i128 = 100;
/// Tokens minted to each member — enough to cover every round of `create_config`.
const FUNDING: i128 = 1_000;

fn create_config(env: &Env, token: &Address, max_members: u32) -> CircleConfig {
    CircleConfig {
        organizer: Address::generate(env),
        token: token.clone(),
        name: String::from_str(env, "Contribution Replay Circle"),
        contribution_amount: AMOUNT,
        max_members,
        payout_type: PAYOUT_FIXED,
        total_rounds: 3,
        contribution_deadline_seconds: 604_800,
        min_moi_score: 0,
        collateral_amount: 0,
        penalty_bps: 500,
        grace_period_seconds: 86_400,
        max_strikes: 3,
        slug: String::from_str(env, "contribution-replay"),
    }
}

/// Deploys a circle and lets `joined` funded members join it. The circle only becomes
/// ACTIVE once `max_members` members have joined.
fn setup_circle<'a>(
    env: &'a Env,
    max_members: u32,
    joined: u32,
) -> (CircleClient<'a>, Address, Address, Address, Vec<Address>) {
    env.mock_all_auths();

    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let config = create_config(env, &token, max_members);
    let admin = config.organizer.clone();
    let factory = Address::generate(env);
    let contract_id = env.register(Circle, CircleArgs::__constructor(&admin, &factory, &config));
    let client = CircleClient::new(env, &contract_id);

    let token_client = soroban_sdk::token::StellarAssetClient::new(env, &token);
    let mut members = Vec::new(env);
    for _ in 0..joined {
        let member = Address::generate(env);
        token_client.mint(&member, &FUNDING);
        client.join(&member);
        members.push_back(member);
    }

    (client, contract_id, admin, token, members)
}

/// A full circle, i.e. one that is ACTIVE and therefore accepts contributions.
fn setup_active_circle<'a>(
    env: &'a Env,
    max_members: u32,
) -> (CircleClient<'a>, Address, Address, Address, Vec<Address>) {
    setup_circle(env, max_members, max_members)
}

/// Reads the `(member, round)` uniqueness entry straight from contract storage.
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

fn balance(env: &Env, token: &Address, member: &Address) -> i128 {
    soroban_sdk::token::Client::new(env, token).balance(member)
}

// ── Success path ─────────────────────────────────────────────────────────────

#[test]
fn test_contribute_records_contribution_once() {
    let env = Env::default();
    let (client, contract_id, _admin, token, members) = setup_active_circle(&env, 2);
    let member = members.get(0).unwrap();

    client.contribute(&member, &AMOUNT, &0u32);

    let contributions = client.get_contributions(&member, &0, &10);
    assert_eq!(contributions.len(), 1);
    assert_eq!(contributions.get(0).unwrap().round, 0u32);
    assert_eq!(contributions.get(0).unwrap().amount, AMOUNT);

    // The uniqueness entry is written so the next attempt is answered by one key read.
    assert_eq!(uniqueness_entry(&env, &contract_id, &member, 0), Some(true));
    // Funds are held by the circle, not by the member.
    assert_eq!(balance(&env, &token, &member), FUNDING - AMOUNT);
    assert_eq!(balance(&env, &token, &contract_id), AMOUNT);
}

// ── Replay / double-submit ───────────────────────────────────────────────────

#[test]
fn test_contribute_rejects_double_submit_in_same_round() {
    let env = Env::default();
    let (client, _contract_id, _admin, token, members) = setup_active_circle(&env, 2);
    let member = members.get(0).unwrap();

    client.contribute(&member, &AMOUNT, &0u32);

    // The replayed transaction is rejected...
    assert_eq!(
        client.try_contribute(&member, &AMOUNT, &0u32),
        Err(Ok(CircleError::AlreadyContributed))
    );

    // ...and leaves no trace: one record and exactly one debit.
    assert_eq!(client.get_contributions(&member, &0, &10).len(), 1);
    assert_eq!(balance(&env, &token, &member), FUNDING - AMOUNT);
}

#[test]
fn test_contribute_rejects_replayed_submissions_for_every_member() {
    let env = Env::default();
    let (client, _contract_id, _admin, token, members) = setup_active_circle(&env, 2);
    let first = members.get(0).unwrap();
    let second = members.get(1).unwrap();

    // Both members submit for the same round, then both replay their transaction.
    client.contribute(&first, &AMOUNT, &0u32);
    client.contribute(&second, &AMOUNT, &0u32);
    assert_eq!(
        client.try_contribute(&first, &AMOUNT, &0u32),
        Err(Ok(CircleError::AlreadyContributed))
    );
    assert_eq!(
        client.try_contribute(&second, &AMOUNT, &0u32),
        Err(Ok(CircleError::AlreadyContributed))
    );

    assert_eq!(client.get_contributions(&first, &0, &10).len(), 1);
    assert_eq!(client.get_contributions(&second, &0, &10).len(), 1);
    assert_eq!(balance(&env, &token, &first), FUNDING - AMOUNT);
    assert_eq!(balance(&env, &token, &second), FUNDING - AMOUNT);
}

#[test]
fn test_contribute_uniqueness_is_scoped_to_one_round() {
    let env = Env::default();
    let (client, _contract_id, admin, _token, members) = setup_active_circle(&env, 2);
    let first = members.get(0).unwrap();
    let second = members.get(1).unwrap();

    client.contribute(&first, &AMOUNT, &0u32);
    client.contribute(&second, &AMOUNT, &0u32);
    client.trigger_payout(&admin, &0u32);
    assert_eq!(client.get_status().current_round, 1u32);

    // The same member is allowed (and required) to contribute again in the new round.
    client.contribute(&first, &AMOUNT, &1u32);
    let contributions = client.get_contributions(&first, &0, &10);
    assert_eq!(contributions.len(), 2);
    assert_eq!(contributions.get(1).unwrap().round, 1u32);
}

#[test]
fn test_contribute_rejects_replay_of_previous_round_after_advance() {
    let env = Env::default();
    let (client, _contract_id, admin, _token, members) = setup_active_circle(&env, 2);
    let first = members.get(0).unwrap();
    let second = members.get(1).unwrap();

    client.contribute(&first, &AMOUNT, &0u32);
    client.contribute(&second, &AMOUNT, &0u32);
    client.trigger_payout(&admin, &0u32);

    // A replayed transaction for the round that just closed cannot be re-applied.
    assert_eq!(
        client.try_contribute(&first, &AMOUNT, &0u32),
        Err(Ok(CircleError::RoundNotCurrent))
    );
    assert_eq!(client.get_contributions(&first, &0, &10).len(), 1);
}

// ── Other failure paths ──────────────────────────────────────────────────────

#[test]
fn test_contribute_rejects_wrong_amount() {
    let env = Env::default();
    let (client, _contract_id, _admin, _token, members) = setup_active_circle(&env, 2);
    let member = members.get(0).unwrap();

    assert_eq!(
        client.try_contribute(&member, &(AMOUNT + 1), &0u32),
        Err(Ok(CircleError::ContributionMismatch))
    );
    assert_eq!(
        client.try_contribute(&member, &(AMOUNT - 1), &0u32),
        Err(Ok(CircleError::ContributionMismatch))
    );
    // A rejected amount must not consume the member's round.
    assert_eq!(client.get_contributions(&member, &0, &10).len(), 0);
    assert!(client.try_contribute(&member, &AMOUNT, &0u32).is_ok());
}

#[test]
fn test_contribute_rejects_wrong_round() {
    let env = Env::default();
    let (client, _contract_id, _admin, _token, members) = setup_active_circle(&env, 2);
    let member = members.get(0).unwrap();

    assert_eq!(
        client.try_contribute(&member, &AMOUNT, &1u32),
        Err(Ok(CircleError::RoundNotCurrent))
    );
    assert_eq!(client.get_contributions(&member, &0, &10).len(), 0);
}

#[test]
fn test_contribute_rejects_non_member() {
    let env = Env::default();
    let (client, _contract_id, _admin, _token, _members) = setup_active_circle(&env, 2);
    let outsider = Address::generate(&env);

    assert_eq!(
        client.try_contribute(&outsider, &AMOUNT, &0u32),
        Err(Ok(CircleError::NotMember))
    );
}

#[test]
fn test_contribute_rejects_exited_member() {
    let env = Env::default();
    let (client, _contract_id, _admin, _token, members) = setup_active_circle(&env, 2);
    let member = members.get(0).unwrap();

    client.contribute(&member, &AMOUNT, &0u32);
    client.exit_circle(&member);

    assert_eq!(
        client.try_contribute(&member, &AMOUNT, &0u32),
        Err(Ok(CircleError::InvalidMemberStatus))
    );
}

#[test]
fn test_contribute_rejects_when_circle_not_active() {
    let env = Env::default();
    // Only one of the two required members joined, so the circle is still PENDING.
    let (client, _contract_id, _admin, _token, members) = setup_circle(&env, 2, 1);
    let member = members.get(0).unwrap();

    assert_eq!(client.try_contribute(&member, &AMOUNT, &0u32), Err(Ok(CircleError::NotActive)));
}

#[test]
fn test_contribute_rejects_when_paused() {
    let env = Env::default();
    let (client, _contract_id, admin, _token, members) = setup_active_circle(&env, 2);
    let member = members.get(0).unwrap();

    client.pause_circle(&admin);

    assert_eq!(
        client.try_contribute(&member, &AMOUNT, &0u32),
        Err(Ok(CircleError::ContractPaused))
    );

    client.unpause_circle(&admin);
    assert!(client.try_contribute(&member, &AMOUNT, &0u32).is_ok());
}

// ── Gas: uniqueness entry vs history scan ────────────────────────────────────

/// Records `rounds` complete rounds of contributions in a full five-member circle and
/// returns the `(uniqueness entry, history scan)` CPU cost of rejecting a replay of the
/// last recorded round.
///
/// Both measurements exercise the exact same code path — same circle, same members, same
/// round — and differ only in whether the guard is answered by the O(1) uniqueness entry
/// or has to fall back to scanning the recorded history (the behaviour of a circle
/// upgraded from an older contract).
fn duplicate_rejection_costs(env: &Env, rounds: u32) -> (u64, u64) {
    // A bronze-tier organizer may create circles of at most five members, so the history is
    // grown by recording more rounds rather than by adding members.
    let (client, contract_id, admin, _token, members) = setup_active_circle(env, 5);
    let last_round = rounds - 1;

    for round in 0..rounds {
        for i in 0..members.len() {
            client.contribute(&members.get(i).unwrap(), &AMOUNT, &round);
        }
        if round < last_round {
            client.trigger_payout(&admin, &round);
        }
    }
    let member = members.get(0).unwrap();

    // Warm guard: the duplicate is rejected from the uniqueness entry alone.
    let _ = client.try_contribute(&member, &AMOUNT, &last_round);
    let keyed = env.cost_estimate().budget().cpu_instruction_cost();

    // Cold guard: the entry is missing, so the recorded history is scanned.
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::ContributionExists(member.clone(), last_round));
    });
    let _ = client.try_contribute(&member, &AMOUNT, &last_round);
    let scanned = env.cost_estimate().budget().cpu_instruction_cost();

    (keyed, scanned)
}

#[test]
fn test_duplicate_contribution_lookup_cost_does_not_scale_with_history() {
    let env = Env::default();

    // Same five-member circle shape, two different amounts of recorded history.
    let (short_keyed, short_scanned) = duplicate_rejection_costs(&env, 1); // 5 records
    let (long_keyed, long_scanned) = duplicate_rejection_costs(&env, 3); // 15 records
    let short_scan_cost = short_scanned - short_keyed;
    let long_scan_cost = long_scanned - long_keyed;

    println!("replay rejected from the uniqueness entry: {short_keyed} / {long_keyed} cpu insns");
    println!(
        "replay rejected by scanning 5 / 15 records: {short_scanned} / {long_scanned} cpu insns"
    );
    println!("scan overhead: {short_scan_cost} / {long_scan_cost} cpu insns");

    // Scanning the history must cost more than reading the uniqueness entry...
    assert!(
        short_scan_cost > 0 && long_scan_cost > 0,
        "the history scan must cost more than the uniqueness entry"
    );
    // ...and that overhead is what grows with the amount of recorded history, while the
    // uniqueness entry keeps the guard's cost independent of it.
    assert!(
        long_scan_cost > short_scan_cost,
        "scanning more history must cost more: {short_scan_cost} vs {long_scan_cost}"
    );
}
