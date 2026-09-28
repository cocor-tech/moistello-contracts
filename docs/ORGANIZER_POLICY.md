# Organizer Conflict of Interest Policy (#335)

## Problem
The organizer can join and contribute to their own circle. This creates a conflict of
interest where the organizer can manipulate rounds, votes, and payouts (especially
for PAYOUT_FIXED, PAYOUT_AUCTION, PAYOUT_VOTE where position/bid/vote determines winner).

## Decision — Option 1: Disallow organizer from joining (configurable per circle)

Default: **organizer MAY NOT join their own circle**. Any call to `join` or
`batch_invite` where `member == circle.organizer` fails with
`CircleError::OrganizerCannotJoin = 40`.

Per-circle override: admin may allow organizer participation for a specific circle
via admin-only entry point:

```rust
set_allow_organizer_join(env, admin, allow: bool)
get_allow_organizer_join(env) -> bool  // default false
```

Storage: `DataKey::AllowOrganizerJoin` (instance storage, bool).

### Alternative considered — Option 2: Cap organizer contribution count
Capping contributions does not remove the conflict for payout manipulation
(the organizer still influences payout resolution even with 1 contribution).
Therefore Option 1 with an explicit opt-in is preferred.

## Implementation
- `packages/circle/src/contract.rs:join` — checks `member == circle.organizer && !allow` before scoring/allowlist checks.
- `packages/circle/src/contract.rs:batch_invite` — same check per member with early atomic revert.
- `packages/circle/src/types.rs` — new error variant `OrganizerCannotJoin = 40` and DataKey `AllowOrganizerJoin`.
- `packages/circle/src/contract.rs:set_allow_organizer_join` / `get_allow_organizer_join` — admin-only setter.

## Tests required (#335 AC)
- `test_organizer_cannot_join_default` — organizer join fails with OrganizerCannotJoin.
- `test_organizer_can_join_when_allowed` — after admin sets allow=true, organizer join succeeds.
- `test_batch_invite_rejects_organizer` — batch containing organizer fails atomically.
- `test_organizer_contribute_blocked_via_join` — organizer who failed to join cannot contribute.
- `test_set_allow_organizer_unauthorized` — non-admin cannot toggle flag.

## Gas
Single instance-storage read per join/batch_invite (bool), negligible vs member list reads.

## Upgrade
Existing circles default to `false` (not set → not allowed), preserving stricter security post-upgrade without migration.
