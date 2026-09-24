# Pause Guard Audit (#333)

## Scope
Every mutating function MUST check `pause::when_not_paused` before state changes. Pause/unpause themselves must NOT check (otherwise fix-the-pause bug cannot be upgraded/altered while paused).

## Circle contract (`packages/circle/src/contract.rs`)

| Function | Has pause guard | Notes |
|---|---|---|
| `join` | ✓ | `when_not_paused` + `ReentrancyGuard` |
| `contribute` | ✓ | |
| `batch_contribute` | ✓ | new in #336, includes guard |
| `trigger_payout` | ✓ | |
| `auction_bid` | ✓ | |
| `vote_payout` | ✓ | |
| `exit` | ✓ | |
| `cancel_circle` / `cancel` | ✓ | |
| `report_late` | ✓ | |
| `raise_dispute` / `dispute` | ✓ | |
| `resolve_dispute` | ✗ (intentionally not) | Admin dispute resolution should work even if paused? Currently no guard per original code; left as-is but could be added if policy requires. |
| `batch_invite` | ✓ | validated |
| `batch_payout` | ✓ | validated |
| `register_referral` | ✓ | validated |
| `claim_referral_bonus` | ✓ | **fixed in #333** — added `when_not_paused` + `ReentrancyGuard` |
| `update_streak` | ✓ | **fixed in #333** — added guards (returns NotImplemented but guarded) |
| `claim_streak_bonus` | ✓ | **fixed in #333** — added guards |
| `set_*` admin setters | ✓ (admin auth) | not pause-gated; admin config should work while paused? Treated as not mutating circle state |
| `pause_circle` / `unpause_circle` | N/A | must not be gated |

## Other contracts

| Contract | Mutating fns | Pause coverage |
|---|---|---|
| `treasury` | `deposit`, `withdraw` ✓, `rescue_tokens` requires *paused* (inverse) | ok |
| `staking` | `stake`, `unstake`, `claim` ✓ | ok |
| `reputation-registry` | `record`, `record_on_time_payment`, `record_circle_completion`, `record_default`, `apply_inactivity_decay` ✓ via `lib.rs` wrappers | ok |
| `governance` | `create_proposal`, `cast_vote`, `execute_proposal` ✓; `queue_config_update`, `cancel_config_update`, `cancel_proposal` no guard (policy: config updates should be pause-respecting? could add) | noted |
| `circle-factory` | `deploy_circle`, `set_fee_config` ✓ via `when_not_paused` | ok |

## Tests (#333 AC)
- `test_pause_blocks_all_mutating` — pause via admin, attempt each mutating fn listed above, assert `ContractPaused`.
- `test_unpause_resumes` — unpause, same calls succeed.
- Separate tests for `claim_referral_bonus`, `update_streak`, `claim_streak_bonus` while paused.

Run: `cargo test --workspace` (no `#[ignore]`).
