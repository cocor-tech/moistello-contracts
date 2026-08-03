# Soroban Contract Upgrade & Migration Guide

This document describes how to safely upgrade a deployed Moistello contract
(`circle`, `circle-factory`, `reputation-registry`, `treasury`, `escrow-swap`,
`staking`, `governance`, `governance-token`) in place, without losing existing
on-chain state.

Soroban upgrades a contract by replacing the Wasm bytecode bound to a contract
ID (`env.deployer().update_current_contract_wasm`); the contract's storage
(instance/persistent/temporary entries) is untouched by the upgrade itself.
That means an upgrade is safe *only* if the new Wasm reads existing storage in
a way that's compatible with how the old Wasm wrote it. Almost every real
incident with contract upgrades traces back to a storage-layout mismatch, not
the deploy step — so most of this guide is about that.

## Prerequisite: wire up the `upgrade` entry point

`packages/common/src/upgrade.rs` already implements the upgrade primitives
(`set_implementation`, `upgrade_contract`, `get_implementation`), and
`scripts/deploy-upgrade.sh --upgrade-only` already assumes each contract
exposes a public `upgrade(admin, new_wasm_hash)` entry point that calls
`common::upgrade::upgrade_contract`. As of this writing, **no contract's
`lib.rs` actually exposes that entry point** — `common::upgrade` is wired into
none of `#[contractimpl]` blocks. Before this guide's upgrade procedure can be
executed against a real deployment, each contract needs something like:

```rust
pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) -> Result<(), YourError> {
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin)
        .ok_or(YourError::NotInitialized)?;
    if admin != stored_admin { return Err(YourError::Unauthorized); }
    common::upgrade::upgrade_contract(&env, &admin, &new_wasm_hash)
        .map_err(|_| YourError::Unauthorized)
}
```

Note `upgrade_contract` deliberately does not check `pause::when_not_paused` —
a contract may only be pausable *because* of a bug an upgrade needs to fix, so
the admin must retain the ability to upgrade while paused.

## 1. Storage layout & migration patterns

Every persisted type in this codebase is a `#[contracttype]` struct/enum
(see e.g. `packages/circle/src/types.rs`). Soroban serializes these as XDR
`Map`s keyed by field/variant name, which has two consequences worth
internalizing:

- **Adding a field is the only truly free-form change.** New fields are
  additive: old ledger entries simply lack that key when deserialized under
  the new Wasm, so `Option<T>` fields default to `None`. Non-`Option` fields
  need a concrete default. In this session (see `packages/circle/src/types.rs`
  around `payout_cooldown_seconds`/`last_payout_timestamp` in the fix for
  [#115]), every constructor of the struct (`init`, and any other place that
  builds a fresh `Circle { .. }` literal — e.g. `get_status`'s fallback
  default) has to be updated in lockstep, or the crate won't even compile;
  that's a compile-time safety net for *new* circles, but it does nothing for
  *already-deployed* circles whose stored bytes were written before the field
  existed — see the migration step below for those.
- **Renaming, removing, retyping, or reordering-with-type-changes a field is
  not safe** without an explicit migration. Soroban storage doesn't do schema
  evolution for you; a struct with a renamed or narrowed field will fail to
  deserialize (or worse, silently decode into the wrong shape if the new type
  happens to overlap on the wire) for every ledger entry written before the
  change.

Because upgrading only swaps *code*, not *data*, any of the "not safe" cases
above requires an explicit **migration step**: a one-time admin-only function
in the new Wasm that reads the old shape, transforms it, and writes the new
shape, run once right after the upgrade and before any other entry point
touches that storage key. Pattern:

```rust
// New contract version, one-time migration entry point.
pub fn migrate_v2(env: Env, admin: Address) -> Result<(), CircleError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin)
        .ok_or(CircleError::NotInitialized)?;
    if admin != stored_admin { return Err(CircleError::Unauthorized); }
    if env.storage().instance().get::<_, bool>(&DataKey::MigratedV2).unwrap_or(false) {
        return Ok(()); // idempotent — safe to call twice
    }
    // ... read old-shape data, write new-shape data ...
    env.storage().instance().set(&DataKey::MigratedV2, &true);
    Ok(())
}
```

Guidelines:
- Gate every migration with a "have I already run" flag in instance storage
  (as above) so a retried/duplicate invocation is a no-op, not a double
  transform.
- Prefer additive changes over migrations whenever the feature allows it —
  a migration is inherently a small, custom piece of one-off code that only
  runs once and is easy to under-test.
- If a `persistent()` collection (e.g. `Vec<Member>`, `Vec<Contribution>`)
  needs a shape change, migrate it in the same pass rather than lazily
  upgrading entries on read — lazy upgrade-on-read doubles the number of code
  paths that need to handle both shapes indefinitely.

## 2. Pre-upgrade checklist

Before touching a live network:

1. `cargo test --workspace` — all existing tests, including the target
   contract's, must pass against the new code.
2. `cargo clippy -- -D warnings` — matches what CI is configured to run (see
   the caveat below).
3. `cargo build --target wasm32-unknown-unknown --release --workspace` then
   `scripts/check-wasm-size.sh target/wasm32-unknown-unknown/release` — the
   new Wasm must stay inside the size budget.
4. Diff the `#[contracttype]` definitions for the contract being upgraded
   against the version currently deployed (`git diff <deployed-tag> --
   packages/<contract>/src/types.rs`). Classify every change per §1: additive
   (safe), or requires a migration (write one, see §1).
5. Write or update a storage-compatibility test (see §3) covering the
   specific fields that changed.

> **CI caveat:** `.github/workflows/*.yml` currently triggers on
> `branches: ["main"]`, but this repository's default branch is `master` —
> so none of the above (`cargo test`, `clippy`, the wasm size check) are
> actually running automatically on pushes/PRs today. Run them locally as
> part of this checklist until that's corrected, and don't rely on a green
> GitHub Actions tab as a signal.

## 3. Testing the upgrade itself, not just the new code

Passing unit tests for the new contract version tells you the new code is
correct in isolation — it does not tell you the new code correctly reads
storage that the *old* code wrote. Test that explicitly, using
`env.register` to bind an already-populated ledger snapshot, or by driving
both versions in-process against the same `Env`:

```rust
#[test]
fn test_upgrade_preserves_existing_circle_state() {
    let env = Env::default();
    env.mock_all_auths();

    // 1. Deploy & exercise the OLD contract shape to populate real state.
    let (client, admin, token) = setup_circle_old_version(&env);
    client.join(&member_one);
    client.join(&member_two);
    // ... contribute, trigger a payout, etc., so persistent storage is
    // non-trivial ...

    // 2. Swap in the NEW Wasm at the same contract ID.
    let new_wasm_hash = env.deployer().upload_contract_wasm(NEW_CIRCLE_WASM);
    client.upgrade(&admin, &new_wasm_hash);

    // 3. Run any migration entry point the new version added.
    client.migrate_v2(&admin);

    // 4. Assert reads through the NEW client still see the OLD data,
    //    and new fields have sane defaults.
    let circle = client.get_status();
    assert_eq!(circle.member_count, 2);
    assert_eq!(circle.payout_cooldown_seconds, 0); // additive field default
}
```

Key things this style of test catches that ordinary unit tests miss:
- A renamed/retyped field that silently deserializes wrong (or panics) for
  pre-upgrade data.
- A migration that isn't actually idempotent (call it twice in the test).
- A new required-auth check on an existing entry point that would break
  clients still calling it the old way.

## 4. Executing the upgrade

`scripts/deploy-upgrade.sh --upgrade-only [--network testnet|mainnet] [--dry-run]`
drives this end-to-end per `scripts/deploy-manifest.json`: it installs each
contract's new Wasm and invokes `upgrade` on the previously-deployed contract
ID (looked up from the latest file in `deployments/`). Always run with
`--dry-run` first to review the exact `stellar` CLI invocations before they
touch a live network.

The manual equivalent, one contract at a time:

```bash
# 1. Build & optimize.
cargo build --target wasm32-unknown-unknown --release -p circle
# (run the workspace's wasm optimization step, if any, before install)

# 2. Install the new Wasm — this returns a wasm hash without touching
#    any existing contract instance.
NEW_HASH=$(stellar contract install \
  --wasm target/wasm32-unknown-unknown/release/circle.wasm \
  --source "$ADMIN_IDENTITY" --network "$NETWORK")

# 3. Point the existing contract ID at the new hash.
stellar contract invoke \
  --id "$CIRCLE_CONTRACT_ID" --source "$ADMIN_IDENTITY" --network "$NETWORK" \
  -- upgrade --new_wasm_hash "$NEW_HASH"

# 4. If this version introduced a migration, run it once, immediately,
#    before any other write touches the contract.
stellar contract invoke \
  --id "$CIRCLE_CONTRACT_ID" --source "$ADMIN_IDENTITY" --network "$NETWORK" \
  -- migrate_v2 --admin "$ADMIN_PUBLIC"

# 5. Verify.
stellar contract invoke \
  --id "$CIRCLE_CONTRACT_ID" --source "$ADMIN_IDENTITY" --network "$NETWORK" \
  -- get_status
```

Record the old wasm hash (from the previous `deployments/*.json` log) before
step 2 — it's what §5 needs for a rollback.

## 5. Rollback plan

Because installing a Wasm hash never deletes a previous one, rolling back is
the same `upgrade` invocation pointed at the previously-installed hash:

```bash
stellar contract invoke \
  --id "$CIRCLE_CONTRACT_ID" --source "$ADMIN_IDENTITY" --network "$NETWORK" \
  -- upgrade --new_wasm_hash "$PREVIOUS_HASH"
```

This is only safe if the version being rolled back **from** did not write any
storage shape the rolled-back-**to** code can't read — i.e. rollback safety
has to be evaluated with the same §1 analysis as a forward upgrade, in
reverse. Concretely:

- If the upgrade being rolled back was purely additive (new fields with
  defaults, new entry points) and no migration ran, rollback is safe: the old
  code simply ignores the extra fields it doesn't know about.
- If a migration ran (§1/§4 step 4) that transformed or deleted old-shape
  data, a naive rollback is **not** safe — the old code will look for data in
  a shape that no longer exists. In that case, rollback requires either a
  reverse migration, or restoring from a pre-migration state snapshot taken
  before step 4.
- Always dry-run + test the specific rollback path in an integration test
  (§3, run in reverse — new-then-old) before doing it against mainnet,
  especially when a migration was involved.

Keep every `deployments/<network>-<timestamp>.json` log file (written by
`scripts/deploy-upgrade.sh`) — it's the audit trail of which wasm hash was
live at each point in time, and what you'd roll back to.
