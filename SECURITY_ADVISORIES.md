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

## RUSTSEC-2026-0576 - Heapless IndexMap::truncate Inconsistent State and Memory Corruption

**Package**: heapless (transitive dependency)

**Severity**: Unsound

**Description**: `IndexMap::truncate` did not rebuild its index structure after removing entries. This left the map in an inconsistent state where the index referenced slots that no longer held valid data, allowing subsequent operations to corrupt memory.

**Status**: Transitive dependency of soroban-sdk@26.0.0 (heapless@0.8.0). Cannot be directly upgraded without soroban-sdk releasing a patched version.

**Mitigation**: Risk accepted with explicit advisory ignore in deny.toml. Monitor soroban-sdk releases for a heapless upgrade that includes the fix.

**Reference**: https://rustsec.org/advisories/RUSTSEC-2026-0576

---

## RUSTSEC-2026-0486 - Rustix xattr Functions Mark Unwritten Bytes as Initialized

**Package**: rustix (transitive dependency)

**Severity**: Unsound

**Description**: The extended-attribute functions in `rustix::fs` (`getxattr`, `lgetxattr`, `fgetxattr`, `listxattr`, `llistxattr`, `flistxattr`) write into a `Buffer` and mark as many bytes initialized as the syscall reported. When given a zero-length buffer, the OS treats the call as a size query and writes nothing, but rustix still advanced the vector's length past its capacity over memory that was never written — undefined behaviour reachable from safe code. Fixed in rustix 1.1.6.

**Status**: Transitive dependency pulled in via the build toolchain (tempfile → rustix). Cannot be directly upgraded without soroban-sdk or its toolchain dependencies releasing a compatible update.

**Mitigation**: Risk accepted with explicit advisory ignore in deny.toml. Monitor soroban-sdk releases for a rustix ≥ 1.1.6 bump.

**Reference**: https://rustsec.org/advisories/RUSTSEC-2026-0486
