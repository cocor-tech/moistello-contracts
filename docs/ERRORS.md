# Contract Error Response Envelope Specification

This document specifies the standard error response envelope for Moistello contracts, simplifying client-side and frontend error parsing across all contract endpoints.

## Error Envelope Schema

All contract errors map deterministically to the standard `ErrorEnvelope`:

```rust
pub struct ErrorEnvelope {
    pub code: u32,
    pub message: String,
    pub details: String,
    pub request_id: u64,
}
```

### Fields

- `code`: The unique numeric error code identifying the failure variant.
- `message`: Canonical human-readable description of the error code.
- `details`: Context-specific error detail (e.g. invalid parameter description, member address, bounds).
- `request_id`: Client-supplied correlation identifier or nonce for tracing and deduplication.

---

## Circle Error Code Mapping

| Code | Variant | Description |
|---|---|---|
| 1 | `NotInitialized` | Contract has not been initialized |
| 2 | `NotActive` | Circle status is not active or action requires active state |
| 3 | `CircleFull` | Maximum member capacity reached |
| 4 | `AlreadyMember` | Member address has already joined the circle |
| 5 | `NotMember` | Address is not an active circle member |
| 6 | `InsufficientMoiScore` | Member MOI reputation score does not meet minimum tier |
| 7 | `RoundNotCurrent` | Specified round does not match contract's current round |
| 8 | `InvalidAmount` | Supplied amount or configuration parameter is out of bounds |
| 9 | `PaymentDeadlinePassed` | Contribution deadline for the round has expired |
| 10 | `MaxStrikesReached` | Member reached maximum allowed default strikes |
| 11 | `NotOrganizer` | Caller is not the registered circle organizer |
| 12 | `ContractPaused` | Contract is currently paused by admin |
| 13 | `InvalidInviteCode` | Supplied invitation code is invalid or expired |
| 14 | `AuctionAlreadyResolved` | Auction payout for this round was already settled |
| 15 | `VoteQuorumNotMet` | Required voting quorum has not been reached |
| 16 | `AlreadyContributed` | Member has already contributed for this round |
| 17 | `AlreadyVoted` | Member has already cast a vote for this round |
| 18 | `AlreadyBidded` | Member has already submitted a bid for this round |
| 19 | `PayoutAlreadyExecuted` | Payout for this round has already been executed |
| 20 | `InvalidPayoutType` | Unknown or unsupported payout strategy |
| 21 | `InvalidRound` | Round index out of range or unrecorded historical round |
| 22 | `ContributionMismatch` | Supplied amount does not equal configured contribution amount |
| 23 | `CircleNotFull` | Circle cannot start until max_members capacity is met |
| 24 | `NotEnoughVotes` | Vote tally insufficient to determine winner |
| 25 | `DisputeAlreadyRaised` | Active dispute is already pending for this circle |
| 26 | `NoActiveDispute` | No open dispute exists to resolve or challenge |
| 27 | `Unauthorized` | Caller lacks permissions for the requested action |
| 28 | `InvalidBid` | Bid discount basis points invalid |
| 29 | `InvalidMemberStatus` | Member is exited, defaulted, or ineligible |
| 30 | `EmptyPayoutOrder` | Payout sequence is unpopulated |
| 31 | `CircleSizeExceedsTier` | Circle size exceeds organizer tier limit |
| 32 | `ContributionExceedsTier` | Contribution amount exceeds organizer tier limit |
| 33 | `VecAccessError` | Vector indexing or collection access error |
| 34 | `AllowlistNotPermitted` | Member is not in the private circle allowlist |
| 35 | `InsufficientContractBalance` | Contract token reserve insufficient for transfer |
| 36 | `SelfReferral` | Referrer cannot refer their own address |
| 37 | `OracleUnavailable` | Price/yield oracle did not respond |
| 38 | `NotImplemented` | Feature variant not implemented |
| 39 | `ZeroPayoutAmount` | Net payout calculates to zero or negative |
| 59 | `PayoutAlreadyScheduled` | Round payout scheduling has begun; contributions locked |

---

## Governance Token Error Code Mapping

| Code | Variant | Description |
|---|---|---|
| 1 | `NotInitialized` | Token contract not initialized |
| 2 | `Unauthorized` | Spender or caller unauthorized |
| 3 | `InsufficientBalance` | Account balance lower than transfer/burn amount |
| 4 | `InvalidAmount` | Amount must be strictly greater than zero |
| 5 | `Overflow` | Numeric addition overflow |
| 6 | `AllowanceExpired` | Spend allowance ledger expired |
| 7 | `AllowanceExceeded` | Transfer amount exceeds approved allowance |
| 8 | `NegativeAllowance` | Allowance cannot be negative |
| 9 | `NotAdmin` | Caller is not token admin |
| 10 | `ContractPaused` | Operations paused |
| 11 | `Underflow` | Numeric subtraction underflow |
| 12 | `Frozen` | Account is frozen by admin |
| 16 | `CannotTransferToSelf` | Transfer, transfer_from, or mint to token contract address rejected |

---

## Client Integration

When invoking contract methods via Soroban RPC, contract error codes are returned in the simulation or invocation result as `Error(Contract, #code)`. Frontend clients should map these codes using `ErrorEnvelope`:

```typescript
export interface ErrorEnvelope {
  code: number;
  message: string;
  details: string;
  request_id: bigint;
}

export function parseContractError(errCode: number, details = "", requestId = 0n): ErrorEnvelope {
  const message = ERROR_CODE_MAP[errCode] ?? "Unknown contract error";
  return { code: errCode, message, details, request_id: requestId };
}
```
