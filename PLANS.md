# Moistello Contracts — Enterprise Implementation Plan

---

## Architecture Overview

```
moistello-contracts/           ← Standalone Rust/Soroban project
moistello-backend/             ← Go API server (references deployed contract IDs)
moistello-frontend/            ← Next.js (calls backend API)

Contracts deploy ONCE to Stellar (immutable).
Backend restarts never touch them.
Backend reads contract IDs/hashes from config — not source code.
```

### Contract Hierarchy

```
 ┌──────────────────────┐
 │   CIRCLE FACTORY     │  Deploys circle instances, tracks registry, manages platform fee
 └──────────┬───────────┘
            │ deploys
 ┌──────────▼───────────┐
 │       CIRCLE          │  Join, contribute, payout (random/fixed/auction/vote), penalties,
 │                       │  collateral, exit, dispute — the core engine
 └──────────┬───────────┘
            │ writes to
 ┌──────────▼───────────┐
 │  REPUTATION REGISTRY  │  MoiScore calculation (0-1000), activity logging, score queries
 └──────────────────────┘

 ┌──────────────────────┐
 │      TREASURY         │  Protocol fee collection (0.5% per payout), admin withdrawal
 └──────────────────────┘


 ┌──────────────────────┐
 │       COMMON          │  VRF, math, access control, emergency pause, upgrade proxy
 └──────────────────────┘
```

### How They Connect

```
Contracts (Stellar testnet)           Backend (Go)
──────────────────────────────       ─────────────────────
factory.DeployCircle(config)    →    backend reads circleFactoryContractId from config
circle.Contribute(amount)       →    backend calls Soroban RPC: invokeContract(contractId, "contribute", args)
events.ContributionReceived     →    indexer picks up on-chain event → writes to PostgreSQL
```

---

---

## PHASE 1 — Smart Contracts + Bindings (Week 1–2)

### 1.1 Rust Workspace Structure

```
contracts/
├── Cargo.toml                     # Workspace: 5 packages + 1 common
├── Makefile                        # Build, test, deploy, optimize, bindings
├── rust-toolchain.toml             # Pin Soroban-compatible Rust version
│
├── packages/
│   ├── circle-factory/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # #[contractimpl]   — single entry point
│   │       ├── contract.rs         # DeployCircle, GetCircles, SetFee
│   │       ├── types.rs            # CircleConfig, FactoryConfig, FeeBps
│   │       ├── events.rs           # CircleDeployed, FeeUpdated
│   │       ├── errors.rs           # FactoryError  (20+ variants)
│   │       ├── storage.rs          # Persistent storage keys + helpers
│   │       ├── auth.rs             # Admin-only access control
│   │       └── tests/
│   │           ├── test.rs         # Unit tests: deploy, fee change, access control
│   │           └── integration/
│   │               └── mod.rs      # Multi-contract: factory → circle interaction
│   │
│   ├── circle/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── contract.rs         # Init, Join, Contribute, Payout, Exit, Dispute
│   │       ├── payout/
│   │       │   ├── mod.rs
│   │       │   ├── random.rs       # Soroban PRNG → Fisher-Yates shuffle
│   │       │   ├── fixed.rs        # Predefined order validation
│   │       │   ├── auction.rs      # Chit fund: bid discount → winner → distribution
│   │       │   └── vote.rs         # Weighted voting, quorum check, tiebreaker
│   │       ├── penalties.rs        # LateFee (%), GracePeriod (hours), MaxStrikes, Slashing
│   │       ├── collateral.rs       # Stake, release, slash
│   │       ├── types.rs            # Circle, Member, Round, PayoutType, CircleStatus
│   │       ├── events.rs           # All 12 event types
│   │       ├── errors.rs           # CircleError  (30+ variants)
│   │       ├── storage.rs          # Instance storage layout
│   │       ├── auth.rs             # Member/Organizer-only guards
│   │       └── tests/
│   │           ├── test.rs         # Each function: happy path + edge cases
│   │           └── integration/
│   │               └── mod.rs
│   │
│   ├── reputation-registry/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── contract.rs         # RecordActivity, GetScore, GetHistory
│   │       ├── scoring.rs          # MoiScore algorithm: 35% streaks + 30% completions
│   │       │                        # + 20% volume + 15% recency = 0-1000
│   │       ├── types.rs            # Activity, MoiScore, Tier (Bronze→Diamond)
│   │       ├── events.rs           # ActivityRecorded, ScoreUpdated
│   │       ├── errors.rs
│   │       ├── storage.rs
│   │       └── tests/
│   │
│   ├── treasury/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── contract.rs         # DepositFee, Withdraw (multi-sig), GetBalance
│   │       ├── types.rs
│   │       ├── events.rs
│   │       ├── errors.rs
│   │       ├── auth.rs             # Governance-only withdrawal
│   │       └── tests/
│   │
│   └── common/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── vrf.rs              # Verifiable Random Function (Soroban PRNG wrapper)
│           ├── math.rs             # Fixed-point arithmetic (i128 with 7 decimal places)
│           ├── access.rs           # Role-based access: Admin, Organizer, Member
│           ├── pause.rs            # Emergency pause/unpause (only by admin)
│           └── upgrade.rs          # Upgrade proxy: delegatecall to new implementation
│
├── bindings/                       # Generated Go bindings  (output)
│   └── (generated by soroban-cli or custom tool)
│
├── deploy/
│   ├── testnet.toml                # Testnet: contract IDs, wasm hashes, admin keys
│   └── mainnet.toml                # Mainnet: same structure, real keys
│
└── scripts/
    ├── build.sh                    # Build all packages → wasm  → optimize
    ├── deploy.sh                   # Deploy all contracts to network
    ├── verify.sh                   # Verify deployed contract hashes on-chain
    └── bindings.sh                 # Generate Go bindings from deployed contracts
```

### 1.2 Contract Feature Matrix

| Contract | Functions | Events | Errors | Lines (est.) |
|---|---|---|---|---|
| `circle-factory` | 4 | 2 | 8 | 400 |
| `circle` | 12 | 12 | 30 | 1,200 |
| `reputation-registry` | 3 | 2 | 6 | 350 |
| `treasury` | 3 | 2 | 5 | 250 |
| `common` | — (library) | — | — | 300 |
| **Total** | **22** | **18** | **49** | **~2,500** |

### 1.3 Circle Contract — Core Functions

```rust
// Every function uses #[contractimpl] with strict access control

pub fn initialize(env: Env, config: CircleConfig)           // Called once by factory
pub fn join(env: Env, invite_code: Option<String>)           // Member joins (checks capacity, MoiScore, invite)
pub fn contribute(env: Env, amount: i128, round: u32)        // Record contribution (verifies amount, deadline)
pub fn trigger_payout(env: Env, round: u32)                  // Execute payout for this round
pub fn auction_bid(env: Env, discount_bips: u32)             // Submit auction bid for current round
pub fn vote_payout(env: Env, vote_for: Address)              // Vote for payout recipient
pub fn exit_circle(env: Env)                                 // Early exit with penalty
pub fn report_late(env: Env, member: Address)                // Flag late contribution
pub fn raise_dispute(env: Env, evidence: BytesN<32>)         // Freeze circle, raise dispute
pub fn resolve_dispute(env: Env, resolution: DisputeResolution) // Admin resolves
pub fn get_status(env: Env) -> CircleStatus                  // Read current state
pub fn get_members(env: Env) -> Vec<Member>                  // List all members
```

### 1.4 Payout Logic — All 4 Types

```
Random (VRF):
  env.prng().gen_range(0..totalMembers) → winner position
  Verify winner hasn't received payout yet via bitmap
  Emit PayoutExecuted(winner, amount)

Fixed Order:
  Read position from member list (set during init)
  Round N → member at position N receives payout
  Emit PayoutExecuted(member, amount)

Auction (Chit Fund):
  Members submit discount bids (in basis points, 0-10000)
  Lowest bid (highest discount) wins
  Winner receives: pool_amount - discount_amount
  Discount distributed proportionally to all members as bonus
  Emit AuctionBid + PayoutExecuted

Vote-Based:
  Members cast votes each round (1 vote per member)
  Majority wins. Tiebreaker: earliest joined member
  Quorum check: > 50% must vote
  Emit VoteCast + PayoutExecuted
```

### 1.5 Contract Events (For Indexer to Consume)

```rust
pub enum CircleEvent {
    CircleDeployed      { creator: Address, circle_id: Address, config: CircleConfig },
    MemberJoined        { circle_id: Address, member: Address, position: u32 },
    ContributionReceived { circle_id: Address, member: Address, round: u32, amount: i128 },
    ContributionLate    { circle_id: Address, member: Address, round: u32, penalty: i128 },
    PayoutExecuted      { circle_id: Address, recipient: Address, round: u32, amount: i128, fee: i128 },
    MemberExited        { circle_id: Address, member: Address, penalty: i128 },
    MemberDefaulted     { circle_id: Address, member: Address, strikes: u32 },
    CircleCompleted     { circle_id: Address, total_payouts: i128 },
    CircleCancelled     { circle_id: Address },
    DisputeRaised       { circle_id: Address, member: Address, evidence_hash: BytesN<32> },
    AuctionBid          { circle_id: Address, bidder: Address, discount_bips: u32, round: u32 },
    VoteCast            { circle_id: Address, voter: Address, vote_for: Address, round: u32 },
}
```

### 1.6 Build Pipeline

```makefile
# Makefile targets for Phase 1

.PHONY: build optimize test deploy-bindings bindings clean

build:
	@for pkg in circle-factory circle reputation-registry treasury; do \
		echo "Building $$pkg..."; \
		cargo build --target wasm32-unknown-unknown --release -p $$pkg; \
	done

optimize: build
	soroban contract optimize --wasm target/wasm32-unknown-unknown/release/circle_factory.wasm
	soroban contract optimize --wasm target/wasm32-unknown-unknown/release/circle.wasm
	soroban contract optimize --wasm target/wasm32-unknown-unknown/release/reputation_registry.wasm
	soroban contract optimize --wasm target/wasm32-unknown-unknown/release/treasury.wasm

test:
	cargo test --workspace

test-coverage:
	cargo tarpaulin --workspace --out Html

deploy-testnet: optimize
	@bash scripts/deploy.sh testnet

bindings: deploy-testnet
	@bash scripts/bindings.sh

clean:
	cargo clean
	rm -rf bindings/
```

### 1.7 Soroban SDK Dependencies (Cargo.toml workspace)

```toml
[workspace]
members = [
    "packages/common",
    "packages/circle-factory",
    "packages/circle",
    "packages/reputation-registry",
    "packages/treasury",
]

[workspace.dependencies]
soroban-sdk = "22.2"
soroban-token-sdk = "22.2"

[workspace.package]
version = "0.1.0"
edition = "2021"
```

### 1.8 Testnet Deployment Config (deploy/testnet.toml)

```toml
network = "testnet"
rpc_url = "https://soroban-testnet.stellar.org"
network_passphrase = "Test SDF Network ; September 2015"

[admin]
public_key = "GAX23V3WWDPPR5WRER3KTEUTDLSCGZYMSJY5FDRRKKCIQ4JADF5T27RC"
secret_key = "SDDBM2MKQSV2ZPEDKTSI3IWNEUSJU5DAWW5NSRWNKJ4FABXSYGYW72FO"

[deploy_order]
contracts = ["common", "circle-factory", "treasury", "reputation-registry", "circle"]

[circle-factory]
init_args = { admin = "GAX23V3WWDPPR5WRER3KTEUTDLSCGZYMSJY5FDRRKKCIQ4JADF5T27RC", fee_bps = 50 }
wasm_hash = ""

[treasury]
init_args = { admin = "GAX23V3WWDPPR5WRER3KTEUTDLSCGZYMSJY5FDRRKKCIQ4JADF5T27RC" }

[reputation-registry]
init_args = { admin = "GAX23V3WWDPPR5WRER3KTEUTDLSCGZYMSJY5FDRRKKCIQ4JADF5T27RC" }
```

---

---

## PHASE 2 — Go Contract Client Layer (Week 3–4)

### 2.1 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  CONTRACT CLIENT LAYER   (Go)                │
│                                                              │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐ │
│  │  Transaction    │  │   Account      │  │   Error        │ │
│  │   Builder       │  │   Manager      │  │   Classifier   │ │
│  │                 │  │                │  │                │ │
│  │  Build tx       │  │  Sequence mgmt │  │  Map Soroban   │ │
│  │  Fee estimation │  │  Key management│  │  error codes   │ │
│  │  Simulation     │  │  Multi-sig     │  │  → domain errs │ │
│  │  Submit + retry │  │  Signer mgmt   │  │                │ │
│  │  Poll status    │  └───────┬────────┘  └────────────────┘ │
│  └────────┬────────┘          │                              │
│           │                   │                              │
│  ┌────────▼───────────────────▼───────────────────────────┐  │
│  │              Soroban RPC Client                         │  │
│  │                                                         │  │
│  │  simulateTransaction()     getTransaction()             │  │
│  │  sendTransaction()         getLedgerEntries()           │  │
│  │  getNetwork()              getEvents()                  │  │
│  └─────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │              Generated Contract Bindings                 │  │
│  │                                                         │  │
│  │  NewCircleFactoryClient(network, signer)                 │  │
│  │  factory.DeployCircle(ctx, config) → (contractId, hash)  │  │
│  │  circleClient.Contribute(ctx, amount, round) → txHash    │  │
│  │  circleClient.TriggerPayout(ctx, round) → txHash         │  │
│  │  reputationClient.GetMoiScore(ctx, address) → (score, lv)│  │
│  └─────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Go Package Structure (new files in moistello-backend)

```
moistello-backend/pkg/stellar/
├── client.go              # Horizon + Soroban RPC HTTP client
├── builder.go             # Transaction construction (build → simulate → sign → submit)
├── simulator.go           # Pre-flight simulation + resource estimation
├── submitter.go           # Submit with retry + poll until final + timeout handling
├── signer.go              # Ed25519 signing (uses master keypair from config)
├── account.go             # Account state tracking: sequence numbers, balance, subentries
├── errors.go              # Error classification: Soroban error codes → Go domain errors
│
├── soroban/
│   ├── client.go          # Soroban RPC wrapper (full spec)
│   ├── invoke.go          # Contract invocation helpers (build SorobanOperation)
│   ├── deploy.go          # Contract deployment (WASM upload → create contract)
│   ├── events.go          # Event parsing, filtering by contract ID + topic
│   └── bindings.go        # Generated contract bindings  (auto-generated from deployed contracts)
│
└── bindings/              # Auto-generated from deployed contract IDs  (go generate)
    ├── circle_factory.go
    ├── circle.go
    ├── reputation.go
    └── treasury.go
```

### 2.3 Transaction Flow (Enterprise-Grade)

```go
// Full lifecycle: Build → Simulate → Sign → Submit → Poll

func (c *ContractClient) ExecuteContractCall(
    ctx context.Context,
    contractID string,
    method string,
    args ...scval.Val,
) (string, error) {

    // 1. BUILD the transaction
    tx, err := c.builder.BuildTransaction(ctx, BuildParams{
        SourceAccount:  c.accountMgr.PublicKey(),
        ContractID:     contractID,
        Method:         method,
        Args:           args,
        BaseFee:        100,           // stroops (0.00001 XLM)
        MaxFee:         1000000,       // 0.1 XLM max (safety cap)
        Timeout:        30 * time.Second,
    })
    if err != nil {
        return "", fmt.Errorf("building transaction: %w", err)
    }

    // 2. SIMULATE — catch errors without spending gas
    simResult, err := c.simulator.SimulateTransaction(ctx, tx)
    if err != nil {
        return "", fmt.Errorf("simulation failed: %w", err)
    }
    if simResult.Error != nil {
        // Classify Soroban error into domain error
        return "", c.errorClassifier.Classify(simResult.Error)
    }

    // 3. APPLY simulation results  (resource fees, footprint, auth)
    tx, err = c.simulator.ApplyResources(tx, simResult)
    if err != nil {
        return "", fmt.Errorf("applying simulation: %w", err)
    }

    // 4. SIGN with master keypair
    signedTx, err := c.signer.Sign(tx)
    if err != nil {
        return "", fmt.Errorf("signing transaction: %w", err)
    }

    // 5. SUBMIT with retry logic
    txHash, err := c.submitter.SubmitWithRetry(ctx, signedTx, SubmitRetryConfig{
        MaxAttempts: 3,
        Backoff:     []time.Duration{2 * time.Second, 4 * time.Second, 8 * time.Second},
        Timeout:     45 * time.Second,
    })
    if err != nil {
        return "", fmt.Errorf("submitting transaction: %w", err)
    }

    // 6. POLL until final (success or permanent failure)
    result, err := c.submitter.PollUntilFinal(ctx, txHash, PollConfig{
        Interval:  2 * time.Second,
        Timeout:   60 * time.Second,
    })
    if err != nil {
        return txHash, fmt.Errorf("transaction timed out: %w", err)
    }
    if !result.Successful {
        return txHash, fmt.Errorf("transaction failed: %s", result.ResultXDR)
    }

    return txHash, nil
}
```

### 2.4 Account Sequence Manager

```go
// Prevents sequence number gaps (critical for enterprise reliability)
type AccountManager struct {
    mu          sync.Mutex
    publicKey   string
    currentSeq  int64
    lastFetched time.Time
    horizon     *Client
    maxDrift    time.Duration  // refresh from chain if older than this
}

func NewAccountManager(horizon *Client, publicKey string) *AccountManager {
    return &AccountManager{
        horizon:   horizon,
        publicKey: publicKey,
        maxDrift:  30 * time.Second,
    }
}

// NextSequence returns the next valid sequence number.
// Thread-safe. Refreshes from chain if local state is stale.
func (m *AccountManager) NextSequence(ctx context.Context) (int64, error) {
    m.mu.Lock()
    defer m.mu.Unlock()

    // Refresh from chain if stale (catches external transactions)
    if m.lastFetched.IsZero() || time.Since(m.lastFetched) > m.maxDrift {
        seq, err := m.horizon.GetSequence(ctx, m.publicKey)
        if err != nil {
            return 0, fmt.Errorf("fetching sequence from chain: %w", err)
        }
        m.currentSeq = seq
        m.lastFetched = time.Now()
    }

    seq := m.currentSeq
    m.currentSeq++
    return seq, nil
}

// Reset forces a chain refresh on the next NextSequence call
func (m *AccountManager) Reset() {
    m.mu.Lock()
    m.lastFetched = time.Time{}
    m.mu.Unlock()
}
```

### 2.5 Generated Bindings Pattern

```go
// Example: bindings/circle_factory.go  (generated)
type CircleFactoryClient struct {
    contractID string
    network    string
    client     *soroban.Client
    signer     *Signer
}

func NewCircleFactoryClient(
    network string,
    contractID string,
    client *soroban.Client,
    signer *Signer,
) *CircleFactoryClient {
    return &CircleFactoryClient{contractID, network, client, signer}
}

func (c *CircleFactoryClient) DeployCircle(
    ctx context.Context,
    admin Address,
    config CircleConfig,
) (Address, string, error) {
    txHash, err := c.client.ExecuteContractCall(ctx, BuildParams{
        ContractID:  c.contractID,
        Method:      "deploy_circle",
        Args:        []scval.Val{admin.ToSCVal(), config.ToSCVal()},
        SourceAccount: c.signer.PublicKey(),
    })
    if err != nil {
        return "", "", err
    }

    // Parse the return value (new circle contract address)
    result, err := c.client.GetTransactionResult(ctx, txHash)
    if err != nil {
        return "", "", err
    }

    circleAddr, err := result.GetContractAddress(0)
    return circleAddr, txHash, err
}
```

---

---

## PHASE 3 — Indexer Engine (Week 5–6)

### 3.1 Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    INDEXER ENGINE                         │
│                                                           │
│  ┌─────────────────┐       ┌─────────────────┐           │
│  │   Poll Loop      │       │   Reconciler    │           │
│  │   (every 3s)     │       │   (every 5m)    │           │
│  │                  │       │                 │           │
│  │  Read cursor     │       │  Scan all our   │           │
│  │  from DB         │       │  contract IDs   │           │
│  │  ↓               │       │  for events     │           │
│  │  GET /ledgers    │       │  since last     │           │
│  │  ?cursor=X&limit │       │  snapshot       │           │
│  │  ↓               │       │  ↓              │           │
│  │  For each ledger:│       │  Find gaps →    │           │
│  │   GET /txns      │       │  replay missed  │           │
│  │   Filter ours    │       │  ledgers        │           │
│  │   ↓              │       │                 │           │
│  │  Decode events   │       │                 │           │
│  │   ↓              │       │                 │           │
│  │  Process → DB    │       │                 │           │
│  └────────┬─────────┘       └────────┬────────┘           │
│           │                          │                    │
│           ▼                          ▼                    │
│  ┌────────────────────────────────────────────┐           │
│  │            EVENT PROCESSOR                  │           │
│  │                                             │           │
│  │  Soroban event → Domain event mapping       │           │
│  │  Deduplication by txn_hash                  │           │
│  │  Idempotent DB writes (INSERT ON CONFLICT)  │           │
│  │  WebSocket broadcast (real-time push)       │           │
│  │  RabbitMQ publish (async workers)            │           │
│  └────────────────────────────────────────────┘           │
│                                                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐   │
│  │  Cursor   │  │   Dead   │  │      Metrics          │   │
│  │  Tracker  │  │  Letter  │  │  (Prometheus)         │   │
│  │  (in DB)  │  │  Queue   │  │                       │   │
│  │           │  │  (Redis) │  │  events_processed      │   │
│  │ last_ledger│ │          │  │  events_failed         │   │
│  │ processed │  │  Failed  │  │  cursor_lag            │   │
│  │  _at      │  │  events  │  │  reconciler_runs       │   │
│  └──────────┘  │  retry N │  │  db_write_latency       │   │
│                └──────────┘  └──────────────────────┘   │
└──────────────────────────────────────────────────────────┘
```

### 3.2 Indexer Files

```
moistello-backend/internal/indexer/
├── engine.go           # Main event loop + goroutine management
├── poller.go           # Horizon ledger polling
├── processor.go        # Event → DB + WS + RMQ
├── cursor.go           # Cursor read/write from DB
├── reconciler.go       # Gap detection + replay
├── deduplicator.go     # Transaction hash dedup (in-memory LRU + DB fallback)
└── metrics.go          # Prometheus metrics
```

### 3.3 Event Processor — Mapping

```go
func (p *EventProcessor) processEvent(e SorobanEvent) error {
    // Deduplicate
    if p.dedup.Has(e.TransactionHash) {
        return nil
    }
    p.dedup.Add(e.TransactionHash)

    // Map to domain event
    switch e.Topic {

    // ── Circle Events ──
    case "CircleDeployed":
        return p.db.CreateCircle(ctx, &circle.Circle{
            ContractID:  e.Data["circle_id"],
            Name:        e.Data["config"].CircleConfig.Name,
            OrganizerID: e.Data["creator"],
            Status:      "pending",
        })

    case "MemberJoined":
        return p.db.CreateMember(ctx, &circle.CircleMember{
            CircleID: e.Data["circle_id"],
            UserID:   e.Data["member"],
            Position: e.Data["position"],
            Status:   "active",
        })

    case "ContributionReceived":
        return p.db.CreateContribution(ctx, &contrib.Contribution{
            CircleID:    e.Data["circle_id"],
            UserID:      e.Data["member"],
            RoundNumber: e.Data["round"],
            Amount:      e.Data["amount"],
            TxnHash:     e.TransactionHash,
        })

    case "PayoutExecuted":
        return p.db.CreatePayout(ctx, &payout.Payout{
            CircleID:    e.Data["circle_id"],
            RecipientID: e.Data["recipient"],
            RoundNumber: e.Data["round"],
            Amount:      e.Data["amount"],
            FeeAmount:   e.Data["fee"],
            TxnHash:     e.TransactionHash,
            PayoutType:  "random", // derived from circle config
        })

    // ── Reputation Events ──
    case "ActivityRecorded":
        // Trigger MoiScore recalculation
        go p.reputationSvc.UpdateScore(ctx, e.Data["user_id"])

    // ── Remaining events mapped similarly ──
    case "ContributionLate", "MemberExited", "MemberDefaulted",
         "CircleCompleted", "CircleCancelled", "DisputeRaised",
         "AuctionBid", "VoteCast":
        return p.handleGenericEvent(ctx, e)

    default:
        p.log.Warn().Str("topic", e.Topic).Msg("unknown event topic")
        return nil
    }

    // Broadcast real-time
    p.wsHub.Broadcast(circleID, e)

    // Publish for async workers
    p.rmq.Publish("moistello.events", e.Topic, e)

    // Update Prometheus
    p.metrics.EventsProcessed.Inc()

    return nil
}
```

--- 

---

## PHASE 4 — Production Hardening (Week 7–8)

### 4.1 Contract Upgrade Pattern

```rust
// Upgradeable contracts via proxy pattern

// Proxy contract — never changes
#[contractimpl]
impl Proxy {
    pub fn invoke(env: Env, method: Symbol, args: Vec<Val>) -> Val {
        let impl_address = env.storage().instance().get(&DataKey::Implementation);
        env.invoke_contract(&impl_address, &method, args)
    }
}

// Implementation contract — can be upgraded
#[contractimpl]
impl CircleV1 { /* original logic */ }
impl CircleV2 { /* upgraded logic */ }

// Upgrade process:
// 1. Deploy new implementation
// 2. Admin approves upgrade
// 3. Admin calls proxy.setImplementation(newAddress)
// 4. All state preserved (in proxy storage, not implementation)
```

### 4.2 Emergency Pause

```rust
// common/pause.rs
pub fn pause(env: &Env) {
    admin_only(env);
    env.storage().instance().set(&DataKey::Paused, &true);
}

pub fn unpause(env: &Env) {
    admin_only(env);
    env.storage().instance().set(&DataKey::Paused, &false);
}

pub fn when_not_paused(env: &Env) -> Result<(), ContractError> {
    let paused: bool = env.storage().instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused { Err(ContractError::ContractPaused) } else { Ok(()) }
}

// Usage in every mutating function:
pub fn contribute(env: Env, ...) -> Result<(), ContractError> {
    when_not_paused(&env)?;
    // ...
}
```

### 4.3 Gas Optimization Checklist

| Optimization | Target |
|---|---|
| Minimize storage reads (batch where possible) | -30% gas on contribute |
| Use `env.storage().instance()` over persistent for small data | -20% gas |
| Pre-compute member positions at join time | -15% gas on payout |
| Use bitmaps for tracking completed rounds instead of Vec | -40% storage |
| Remove debug logging in release builds | -10% gas |
| Compress event data (use short field names) | -5% gas |

### 4.4 Load Testing Plan

```yaml
# k6 load test script
scenarios:
  create_circles:
    executor: ramping-vus
    stages:
      - { duration: "1m", target: 10 }    # ramp up
      - { duration: "3m", target: 50 }    # steady load
      - { duration: "1m", target: 0 }     # ramp down
    thresholds:
      - http_req_duration: ["p95<2000"]   # 95th percentile < 2s
      - http_req_failed: ["rate<0.01"]    # < 1% failure rate

  contribute_concurrent:
    executor: constant-vus
    vus: 100                              # 100 simultaneous contributors
    duration: "5m"
    thresholds:
      - txn_confirmation_time: ["p95<10000"]  # 10s for on-chain confirmation
```

### 4.5 Soak Test (24-Hour)

```bash
# Run for 24 hours, monitor:
#  - Sequence gaps (should NOT occur)
#  - Memory usage (should stabilize, not grow)
#  - Event processing lag (should stay < 10 seconds)
#  - DB connection pool saturation
#  - Redis memory usage
#  - Contract state growth
```

### 4.6 Multi-Network Support

```go
type Network string
const (
    Testnet Network = "testnet"
    Mainnet Network = "mainnet"
)

type MultiNetworkClient struct {
    testnet *soroban.Client
    mainnet *soroban.Client
}

func NewMultiNetworkClient(cfg *config.Config) *MultiNetworkClient {
    return &MultiNetworkClient{
        testnet: soroban.NewClient(cfg.Stellar.TestnetRPCURL, cfg.Stellar.TestnetPassphrase),
        mainnet: soroban.NewClient(cfg.Stellar.MainnetRPCURL, cfg.Stellar.MainnetPassphrase),
    }
}

func (m *MultiNetworkClient) For(network Network) *soroban.Client {
    switch network {
    case Mainnet: return m.mainnet
    default:      return m.testnet
    }
}
```

---

---

## PHASE 5 — Delivery Checklist

### Deliverables by Phase

| Phase | Deliverable | Files | Estimated Lines |
|---|---|---|---|
| 1 | Contracts + Bindings | 35 Rust + 5 Go bindings | 2,900 + 500 |
| 2 | Go Contract Client | 15 Go | 2,000 |
| 3 | Indexer Engine | 6 Go | 800 |
| 4 | Hardening + Tests | 25 test files + scripts | 2,500 |
| **Total** | | **~91** | **~8,600** |

### Test Coverage Targets

| Layer | Coverage |
|---|---|
| Contract unit tests | ≥ 95% |
| Contract integration tests | Full lifecycle |
| Go contract client | ≥ 90% |
| Indexer | ≥ 85% |
| End-to-end | Happy paths (create circle → complete) |

### Key Success Criteria

- [ ] All 5 contracts deploy to testnet successfully
- [ ] Full circle lifecycle (create → join → contribute → payout → complete) passes on testnet
- [ ] No sequence gaps during concurrent transaction submission
- [ ] Indexer catches 100% of on-chain events (verified by reconciler)
- [ ] Load test: 100 concurrent contributors, < 1% failure rate, p95 confirmation < 10s
- [ ] Soak test: 24 hours, zero sequence gaps, zero memory leaks
- [ ] Pause/upgrade: contract can be paused and upgraded without losing state
- [ ] All test suites pass: `cargo test` + `go test ./...`
