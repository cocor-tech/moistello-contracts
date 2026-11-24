# escrow-swap — Hash-Time-Locked Escrow Swap

Purpose: Atomic hash-time-locked token swap between two parties. An initiator locks token_a and the responder must reveal a preimage (secret) matching `hash_lock` to claim token_b within `time_lock`, otherwise the swap can be cancelled/refunded. Enables trustless peer-to-peer token exchange without a counterparty risk window beyond the time lock.

## Entry points

Comprehensive documentation for all entry points, including authorization requirements, arguments/units, storage touched, emitted events, and error scenarios, is documented directly in [`src/lib.rs`](src/lib.rs).

- [`__constructor(admin)`](src/lib.rs) — Initialize contract with admin address and empty swap registry (auth: `admin`).
- [`create_swap(initiator, responder, token_a, token_b, initiator_amount, responder_amount, hash_lock, time_lock)`](src/lib.rs) — Create a swap and escrow initiator tokens; returns `u64` swap id (auth: `initiator`).
- [`accept_swap(id, responder, secret)`](src/lib.rs) — Responder accepts by depositing tokens and revealing the secret preimage (auth: `responder`).
- [`complete_swap(id, caller)`](src/lib.rs) — Complete the swap after activation and distribute funds to counterparties (auth: `caller` = initiator or responder).
- [`cancel_swap(id, caller)`](src/lib.rs) — Cancel a pending or active swap and refund deposited tokens (auth: `caller` = initiator or responder).

Reads:
- [`get_swap(id)`](src/lib.rs) — `Result<SwapRequest, EscrowError>` query swap details.
- [`get_swaps()`](src/lib.rs) — `Vec<SwapRequest>` list all swaps.

Pause:
- [`pause(admin)`](src/lib.rs) / [`unpause(admin)`](src/lib.rs) — Emergency stop mechanism (auth: `admin`).

## Storage keys (DataKey)

| Key | Meaning |
|-----|---------|
| `Admin` | admin address |
| `SwapRequests` | stored swap requests |
| `NextSwapId` | u64 next swap id counter |
| `Paused` | pause flag |

## Events

| Event | Topics | Meaning |
|-------|--------|---------|
| `SwapCreated { id, initiator, ... }` | id, initiator | new swap created |
| `SwapAccepted { id, responder }` | id, responder | swap accepted |
| `SwapCompleted { id, initiator, responder }` | id | swap completed |
| `SwapCancelled { id }` | id | swap cancelled |

## Invariants
- Secret must hash to `hash_lock` (`HashLockMismatch`).
- Swap must be active to accept/complete (`SwapNotActive`).
- Cannot accept an already-accepted swap (`AlreadyAccepted`).
- Cannot complete/cancel after time lock expiry without following the correct path (`TimeLockExpired`).
- Sufficient balances required on both sides (`InsufficientBalance`).
- Mutating functions rejected while paused (`ContractPaused`).

## Test command
```bash
# From repo root
cargo test -p escrow-swap
cargo test -p escrow-swap --doc
# Full workspace
cargo test --workspace
```
