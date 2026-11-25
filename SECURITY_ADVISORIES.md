# Known Security Advisories

## RUSTSEC-2026-0581 - Heapless Deque::make_contiguous Broken Invariants

**Package**: heapless (transitive dependency)

**Severity**: Unsound

**Description**: `Deque::make_contiguous` could set the `back` value to either `0` or `N` when it shouldn't, leading to broken invariants and out-of-bounds writes.

**Status**: Transitive dependency of soroban-sdk@26.0.0. Waiting for soroban-sdk to upgrade heapless.

**Mitigation**: Risk accepted with explicit advisory ignore in deny.toml.

**Reference**: https://rustsec.org/advisories/RUSTSEC-2026-0581

---

## RUSTSEC-2026-0574 - Heapless FindMutView::pop Use After Free

**Package**: heapless (transitive dependency)

**Severity**: Unsound

**Description**: `FindMutView::pop` after mutating the value could lead to a use after free.

**Status**: Transitive dependency of soroban-sdk@26.0.0. Waiting for soroban-sdk to upgrade heapless.

**Mitigation**: Risk accepted with explicit advisory ignore in deny.toml.

**Reference**: https://rustsec.org/advisories/RUSTSEC-2026-0574

---

## RUSTSEC-2026-0579 - Heapless Deque TryFrom Invalid State

**Package**: heapless (transitive dependency)

**Severity**: Unsound

**Description**: `Deque::try_from` could produce an invalid state when the input array was as long as the capacity of the Deque.

**Status**: Transitive dependency of soroban-sdk@26.0.0. Waiting for soroban-sdk to upgrade heapless.

**Mitigation**: Risk accepted with explicit advisory ignore in deny.toml.

**Reference**: https://rustsec.org/advisories/RUSTSEC-2026-0579

---

## RUSTSEC-2024-0436 - Paste Crate No Longer Maintained

**Package**: paste (transitive dependency)

**Severity**: Unmaintained

**Description**: The paste crate is no longer maintained. The repository has been archived by its creator.

**Status**: Transitive dependency of soroban-sdk@26.0.0. Potential alternatives include `pastey` or `with_builtin_macros`.

**Mitigation**: Risk accepted with explicit advisory ignore in deny.toml. Monitor for soroban-sdk updates that may replace this dependency.

**Reference**: https://rustsec.org/advisories/RUSTSEC-2024-0436

---

## RUSTSEC-2026-0580 - Heapless Treiber Stack Race Condition

**Package**: heapless (transitive dependency)

**Severity**: Unsound

**Description**: The treiber stack implementation in heapless had a race condition that could allow two threads running concurrently to race, causing one to read from a pointer that had been invalidated by another execution context.

**Status**: This is a transitive dependency of soroban-sdk@26.0.0. Monitor for updates to soroban-sdk that upgrade or replace heapless.

**Mitigation**: 
- Upgrade soroban-sdk when a patched version becomes available
- Consider alternative concurrency patterns if heapless is a critical blocker

**Reference**: https://rustsec.org/advisories/RUSTSEC-2026-0580

---

## RUSTSEC-2026-0577 - Heapless Panicking Drop Implementation Could Lead to UB

**Package**: heapless (transitive dependency via `stellar-strkey@0.0.16`)

**Severity**: Unsound

**Description**: Multiple functions in heapless dropped generic parameters but did not properly handle the unwinding case, leading to broken invariants. A panicking `Drop` implementation can result in undefined behaviour.

**Status**: Transitive dependency of `stellar-strkey@0.0.16`, which is itself a transitive dependency of `soroban-sdk@26.0.0`. Soroban contracts run in a `panic = "abort"` WASM environment (see `Cargo.toml` release profile), so unwinding-based UB cannot be triggered at runtime. Waiting for soroban-sdk to upgrade stellar-strkey/heapless.

**Mitigation**: Risk accepted with explicit advisory ignore in `deny.toml`. The `panic = "abort"` profile in the release build eliminates the unwinding code path that this advisory concerns.

**Reference**: https://rustsec.org/advisories/RUSTSEC-2026-0577

---

## RUSTSEC-2026-0575 - Heapless Spsc::peek Used Relaxed Memory Ordering

**Package**: heapless (transitive dependency via `stellar-strkey@0.0.16`)

**Severity**: Unsound

**Description**: `Spsc::peek` used `Relaxed` memory ordering where `Acquire` was required, creating a potential race condition that could lead to undefined behaviour in concurrent contexts.

**Status**: Transitive dependency of `stellar-strkey@0.0.16`, which is itself a transitive dependency of `soroban-sdk@26.0.0`. Soroban WASM contracts execute in a single-threaded deterministic environment; concurrent SPSC queue access is not possible at contract runtime. Waiting for soroban-sdk to upgrade stellar-strkey/heapless.

**Mitigation**: Risk accepted with explicit advisory ignore in `deny.toml`. The single-threaded WASM execution model means the race condition described in this advisory cannot be triggered in production contract execution.

**Reference**: https://rustsec.org/advisories/RUSTSEC-2026-0575

---

## RUSTSEC-2026-0387 - crypto-bigint Timing Variability in Constant-Time Operations

**Package**: crypto-bigint (transitive dependency via `elliptic-curve@0.13.8` → `k256@0.13.4` → `ecdsa@0.16.9`)

**Severity**: Vulnerability (patched in `>=0.7.6`)

**Description**: Several operations in crypto-bigint that were intended to be constant-time could execute secret-dependent branches depending on the compiler and target CPU. Affected operations include subtraction, zero counting, division, and Montgomery multiplication. Additionally, `UintRef` equality could terminate at the first unequal limb, and `BoxedMontyForm::invert` branched on whether an inverse exists. This could enable timing or microarchitectural side-channel attacks against cryptographic secrets.

**Status**: Locked at `crypto-bigint@0.5.5` as a transitive dependency of `elliptic-curve@0.13.8`, which is pulled in by `k256@0.13.4` and `ecdsa@0.16.9` within `soroban-sdk@26.0.0`. A direct upgrade to `>=0.7.6` would require a breaking semver bump across the entire `elliptic-curve` / `k256` / `ecdsa` chain, which must come from an upstream soroban-sdk release. Waiting for soroban-sdk to upgrade its cryptographic dependencies.

**Mitigation**: Risk accepted with explicit advisory ignore in `deny.toml`. The affected `crypto-bigint` code paths are exercised only during key derivation and signature operations performed by the Stellar SDK host environment, not inside the contract WASM boundary. Contract code does not directly handle raw cryptographic secrets. This issue will be re-evaluated when soroban-sdk ships a release that bumps the `elliptic-curve` dependency tree.

**Reference**: https://rustsec.org/advisories/RUSTSEC-2026-0387
