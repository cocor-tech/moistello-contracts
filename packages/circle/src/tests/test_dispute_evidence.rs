//! Issue #340 — dispute evidence storage with hash verification.
//!
//! `raise_dispute` used to accept any 32-byte value, so an admin resolving a
//! dispute had no way to check that the evidence being claimed actually existed
//! — resolution was an act of trust. The contract now:
//!
//! * refuses an empty (all-zero) commitment, so every dispute commits to
//!   something,
//! * stores the commitment with the raiser and the block timestamp,
//! * exposes `verify_evidence(data)` which re-hashes a candidate preimage and
//!   compares it with the commitment,
//! * exposes `resolve_dispute_with_evidence(resolution, data)`, which makes that
//!   comparison a precondition of resolution.

#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Bytes, BytesN, Env, String};

use crate::types::{CircleConfig, DisputeEntry, STATUS_ACTIVE, STATUS_DISPUTED};
use crate::{Circle, CircleArgs, CircleClient, CircleError};

const CONTRIBUTION: i128 = 100_0000000;

fn build_config(env: &Env, organizer: &Address, token: &Address) -> CircleConfig {
    CircleConfig {
        organizer: organizer.clone(),
        token: token.clone(),
        name: String::from_str(env, "Evidence Circle"),
        contribution_amount: CONTRIBUTION,
        max_members: 2u32,
        payout_type: 1u32,
        total_rounds: 3u32,
        contribution_deadline_seconds: 604_800u64,
        min_moi_score: 0u32,
        collateral_amount: 0i128,
        penalty_bps: 500u32,
        grace_period_seconds: 86_400u64,
        max_strikes: 3u32,
        slug: String::from_str(env, "evidence-circle"),
    }
}

struct Fixture<'a> {
    client: CircleClient<'a>,
    admin: Address,
}

fn setup<'a>(env: &'a Env) -> Fixture<'a> {
    let organizer = Address::generate(env);
    let token_issuer = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(token_issuer).address();
    let config = build_config(env, &organizer, &token);
    let admin = config.organizer.clone();
    let id = env.register(Circle, CircleArgs::__constructor(&admin, &admin, &config));
    let client = CircleClient::new(env, &id);

    let member = Address::generate(env);
    StellarAssetClient::new(env, &token).mint(&member, &CONTRIBUTION);
    client.join(&member);
    Fixture { client, admin }
}

fn evidence(env: &Env, text: &str) -> Bytes {
    Bytes::from_slice(env, text.as_bytes())
}

fn commitment(env: &Env, text: &str) -> BytesN<32> {
    BytesN::from(env.crypto().sha256(&evidence(env, text)))
}

#[test]
fn dispute_rejects_an_empty_evidence_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let fixture = setup(&env);
    let member = Address::generate(&env);
    fixture.client.join(&member);

    let empty = BytesN::from_array(&env, &[0u8; 32]);
    assert_eq!(
        fixture.client.try_raise_dispute(&member, &empty),
        Err(Ok(CircleError::EvidenceRequired))
    );
    // The circle was not frozen by the rejected attempt.
    assert_eq!(fixture.client.get_status().status, STATUS_ACTIVE);
    assert!(fixture.client.get_dispute().is_none());
}

#[test]
fn dispute_stores_raiser_commitment_and_timestamp() {
    let env = Env::default();
    env.mock_all_auths();
    let fixture = setup(&env);
    env.ledger().with_mut(|l| {
        l.timestamp = 1_234_567;
    });

    let raiser = Address::generate(&env);
    fixture.client.join(&raiser);
    let hash = commitment(&env, "chat-log + ledger export");
    fixture.client.raise_dispute(&raiser, &hash);

    let dispute: DisputeEntry = fixture.client.get_dispute().expect("dispute recorded");
    assert_eq!(dispute.raised_by, raiser);
    assert_eq!(dispute.evidence_hash, hash);
    assert_eq!(dispute.raised_at, 1_234_567u64);
    assert_eq!(dispute.resolved_at, 0u64);
    assert_eq!(dispute.resolution, 0u32);
    assert_eq!(fixture.client.get_status().status, STATUS_DISPUTED);
}

#[test]
fn verify_evidence_accepts_the_committed_preimage() {
    let env = Env::default();
    env.mock_all_auths();
    let fixture = setup(&env);
    let raiser = Address::generate(&env);
    fixture.client.join(&raiser);

    let text = "signed statement: round 0 was not paid to me";
    fixture
        .client
        .raise_dispute(&raiser, &commitment(&env, text));

    assert!(fixture.client.verify_evidence(&evidence(&env, text)));
}

#[test]
fn verify_evidence_rejects_tampered_or_wrong_evidence() {
    let env = Env::default();
    env.mock_all_auths();
    let fixture = setup(&env);
    let raiser = Address::generate(&env);
    fixture.client.join(&raiser);

    let text = "amount=100";
    fixture
        .client
        .raise_dispute(&raiser, &commitment(&env, text));

    assert!(!fixture.client.verify_evidence(&evidence(&env, "amount=900")));
    assert!(!fixture.client.verify_evidence(&evidence(&env, "amount=100 ")));
    assert!(!fixture.client.verify_evidence(&Bytes::new(&env)));
    // The commitment itself is unchanged by failed verification attempts.
    assert!(fixture.client.verify_evidence(&evidence(&env, text)));
}

#[test]
fn verify_evidence_without_a_dispute_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let fixture = setup(&env);
    assert_eq!(
        fixture.client.try_verify_evidence(&evidence(&env, "nothing")),
        Err(Ok(CircleError::NoActiveDispute))
    );
}

#[test]
fn resolve_dispute_with_evidence_requires_the_matching_preimage() {
    let env = Env::default();
    env.mock_all_auths();
    let fixture = setup(&env);
    let raiser = Address::generate(&env);
    fixture.client.join(&raiser);

    env.ledger().with_mut(|l| {
        l.timestamp = 42_424;
    });
    let text = "evidence-v1";
    fixture
        .client
        .raise_dispute(&raiser, &commitment(&env, text));

    // Wrong evidence cannot resolve the dispute, and the circle stays frozen.
    assert_eq!(
        fixture
            .client
            .try_resolve_dispute_with_evidence(&fixture.admin, &1u32, &evidence(&env, "evidence-v2")),
        Err(Ok(CircleError::EvidenceMismatch))
    );
    assert_eq!(fixture.client.get_status().status, STATUS_DISPUTED);
    assert_eq!(fixture.client.get_dispute().unwrap().resolution, 0u32);

    // The committed evidence resolves it.
    fixture.client.resolve_dispute_with_evidence(
        &fixture.admin,
        &1u32, // RESOLVE_DISMISS
        &evidence(&env, text),
    );
    assert_eq!(fixture.client.get_status().status, STATUS_ACTIVE);
    let resolved = fixture.client.get_dispute().unwrap();
    assert_eq!(resolved.resolution, 1u32);
    assert_eq!(resolved.resolved_by, fixture.admin);
    assert!(resolved.resolved_at > 0u64);
}

#[test]
fn resolve_dispute_with_evidence_is_admin_only() {
    let env = Env::default();
    env.mock_all_auths();
    let fixture = setup(&env);
    let raiser = Address::generate(&env);
    fixture.client.join(&raiser);

    let text = "evidence-admin-only";
    fixture
        .client
        .raise_dispute(&raiser, &commitment(&env, text));

    let impostor = Address::generate(&env);
    assert_eq!(
        fixture.client.try_resolve_dispute_with_evidence(
            &impostor,
            &1u32,
            &evidence(&env, text)
        ),
        Err(Ok(CircleError::Unauthorized))
    );
    assert_eq!(fixture.client.get_status().status, STATUS_DISPUTED);
}

#[test]
fn legacy_resolve_dispute_refuses_a_dispute_without_evidence() {
    // Disputes raised before the commitment rule existed can hold an all-zero
    // hash. The legacy entry point must refuse to resolve those.
    let env = Env::default();
    env.mock_all_auths();
    let fixture = setup(&env);

    let raiser = Address::generate(&env);
    env.as_contract(&fixture.client.address, || {
        env.storage().persistent().set(
            &crate::types::DataKey::Dispute,
            &DisputeEntry {
                raised_by: raiser.clone(),
                evidence_hash: BytesN::from_array(&env, &[0u8; 32]),
                raised_at: 0u64,
                resolved_at: 0u64,
                resolution: 0u32,
                resolved_by: env.current_contract_address(),
            },
        );
    });

    assert_eq!(
        fixture.client.try_resolve_dispute(&fixture.admin, &1u32),
        Err(Ok(CircleError::EvidenceRequired))
    );
}

#[test]
fn legacy_resolve_dispute_still_works_with_evidence_on_file() {
    let env = Env::default();
    env.mock_all_auths();
    let fixture = setup(&env);
    let raiser = Address::generate(&env);
    fixture.client.join(&raiser);

    fixture
        .client
        .raise_dispute(&raiser, &commitment(&env, "legacy-ok"));

    fixture.client.resolve_dispute(&fixture.admin, &1u32);
    assert_eq!(fixture.client.get_status().status, STATUS_ACTIVE);
}

#[test]
fn a_second_dispute_needs_the_first_one_resolved() {
    let env = Env::default();
    env.mock_all_auths();
    let fixture = setup(&env);
    let raiser = Address::generate(&env);
    fixture.client.join(&raiser);

    fixture
        .client
        .raise_dispute(&raiser, &commitment(&env, "first"));
    assert_eq!(
        fixture
            .client
            .try_raise_dispute(&raiser, &commitment(&env, "second")),
        Err(Ok(CircleError::DisputeAlreadyRaised))
    );
}
