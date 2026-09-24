# Batch Contribution — Design & Guarantees (#336)

## API
```rust
batch_contribute(env: Env, members: Vec<Address>, amounts: Vec<i128>, round: u32) -> Result<(), CircleError>
```

Exposed in `packages/circle/src/lib.rs` as `Circle::batch_contribute`.

## Guarantees
- **Atomic**: if any single contribution fails validation, the entire call reverts. No partial state is committed. Achieved by **validate-then-write**: all inputs are validated before any `token.transfer`, storage write, or event publish occurs. Soroban transaction atomicity ensures revert on `Err`.
- **Validate before state change**: order is `when_not_paused` → `ReentrancyGuard` → load `Circle` once → validate round, status, lengths → validate each member (member existence, active status, amount == `contribution_amount`, not already contributed for round) → then execute transfers/writes.
- **Gas optimization**: single `persistent().get` for `Circle`, `Members`, `Contributions`, and `contribs` map; member status map built once (`Map<Address, u32>`) for O(1) per-member checks; avoids N separate storage reads. Batch size capped at 100 (same as `batch_invite` limit) and requires `members.len() == amounts.len() > 0`.

## Auth
Each `member` in the batch must authorize the call (`member.require_auth()` per entry). Test harness uses `env.mock_all_auths()`.

## Events & Reputation
Per-member `ContributionRecorded` event and reputation callback (`record` on registry or `scoring::record_on_time_payment` fallback) are emitted in execution phase.

## Errors
Reuses existing `CircleError` variants: `InvalidAmount` (length mismatch), `NotActive`, `RoundNotCurrent`, `NotMember`, `InvalidMemberStatus`, `AlreadyContributed`, `ContributionMismatch`, `ContractPaused`.

## Tests (#336 AC)
- `test_batch_contribute_success` — 2 members, correct amounts, round 0 → contributions recorded.
- `test_batch_contribute_partial_failure_atomic` — second amount mismatched → entire batch reverts, zero contributions persisted, balances unchanged.
- `test_batch_contribute_duplicate_in_batch` — already-contributed member in batch → `AlreadyContributed`, no side effects.
- `test_batch_contribute_not_member_rollback` — outsider in batch → `NotMember`, atomic revert.
- `test_batch_contribute_wrong_round` — round != current_round → `RoundNotCurrent`.
- `test_batch_contribute_when_paused` — paused circle → `ContractPaused`.

## Gas budget
Target < 150k for 10-member batch (validated via simulation). Single read path vs N individual `contribute` calls saves ~ N-1 storage reads.
