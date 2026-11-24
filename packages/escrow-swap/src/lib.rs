#![cfg_attr(not(test), no_std)]
mod contract;
#[cfg(test)]
mod test;
mod types;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};

#[contract]
pub struct EscrowSwap;

#[contractimpl]
impl EscrowSwap {
    /// Initializes the EscrowSwap contract with a specified administrator address.
    ///
    /// # Arguments
    /// * `env` - The contract environment.
    /// * `admin` - The address designated as the contract administrator.
    ///
    /// # Authorization
    /// * Requires authorization from `admin`.
    ///
    /// # Storage Touched
    /// * `DataKey::Admin` (instance): Stores the administrator address.
    /// * `DataKey::NextSwapId` (instance): Initializes the swap counter to `0u64`.
    /// * `DataKey::SwapRequests` (persistent): Initializes an empty vector for swap records.
    ///
    /// # Errors
    /// * `EscrowError::AlreadyInitialized` - If the contract has already been initialized.
    /// * `EscrowError::InvalidAdmin` - If `admin` is the contract's own address.
    pub fn __constructor(env: Env, admin: Address) -> Result<(), types::EscrowError> {
        contract::init(&env, &admin)
    }

    /// Creates a new hash-time-locked escrow swap between an initiator and responder.
    ///
    /// Transfers `initiator_amount` of `token_a` from `initiator` into contract escrow.
    ///
    /// # Arguments
    /// * `env` - The contract environment.
    /// * `initiator` - The address creating the swap and depositing `token_a`.
    /// * `responder` - The counterparty address expected to accept the swap.
    /// * `token_a` - Address of the token contract deposited by initiator.
    /// * `token_b` - Address of the token contract required from responder.
    /// * `initiator_amount` - Amount of `token_a` to deposit (must be strictly positive).
    /// * `responder_amount` - Amount of `token_b` required from responder (must be strictly positive).
    /// * `hash_lock` - 32-byte SHA-256 hash digest of the preimage secret required for claim.
    /// * `time_lock` - Unix timestamp deadline in seconds for accepting the swap (must be strictly in the future).
    ///
    /// # Authorization
    /// * Requires authorization from `initiator`.
    ///
    /// # Storage Touched
    /// * `DataKey::NextSwapId` (instance): Increments the sequential swap identifier.
    /// * `DataKey::SwapRequests` (persistent): Appends the new `SwapRequest` record.
    ///
    /// # Events
    /// * Emits `SwapCreated` event (`topics=["swap_new"]`) with swap details.
    ///
    /// # Returns
    /// * `Ok(u64)` - The newly allocated swap ID.
    ///
    /// # Errors
    /// * `EscrowError::ContractPaused` - If the contract is paused.
    /// * `EscrowError::InvalidSwap` - If `initiator` and `responder` are identical.
    /// * `EscrowError::InvalidAmount` - If amounts are non-positive or if ID counter overflows.
    /// * `EscrowError::TimeLockExpired` - If `time_lock <= current ledger timestamp`.
    /// * `EscrowError::InsufficientBalance` - If `initiator` has insufficient token balance.
    /// * `EscrowError::Unauthorized` - If token transfer authorization fails.
    pub fn create_swap(
        env: Env,
        initiator: Address,
        responder: Address,
        token_a: Address,
        token_b: Address,
        initiator_amount: i128,
        responder_amount: i128,
        hash_lock: BytesN<32>,
        time_lock: u64,
    ) -> Result<u64, types::EscrowError> {
        contract::create_swap(
            &env,
            &initiator,
            &responder,
            &token_a,
            &token_b,
            initiator_amount,
            responder_amount,
            hash_lock,
            time_lock,
        )
    }

    /// Accepts a pending escrow swap by revealing the secret matching `hash_lock`.
    ///
    /// Transfers `responder_amount` of `token_b` from `responder` into contract escrow
    /// and activates the swap.
    ///
    /// # Arguments
    /// * `env` - The contract environment.
    /// * `id` - Unique identifier of the pending swap.
    /// * `responder` - Address of the responder accepting the swap (must match swap's responder).
    /// * `secret` - 32-byte preimage whose SHA-256 hash matches the swap's `hash_lock`.
    ///
    /// # Authorization
    /// * Requires authorization from `responder`.
    ///
    /// # Storage Touched
    /// * `DataKey::SwapRequests` (persistent): Updates swap status from `STATUS_PENDING` to `STATUS_ACTIVE`.
    ///
    /// # Events
    /// * Emits `SwapAccepted` event (`topics=["accepted"]`) with `id` and `responder`.
    ///
    /// # Errors
    /// * `EscrowError::ContractPaused` - If the contract is paused.
    /// * `EscrowError::SwapNotFound` - If no swap with `id` exists.
    /// * `EscrowError::AlreadyAccepted` - If the swap status is already `STATUS_ACTIVE`.
    /// * `EscrowError::SwapNotActive` - If the swap status is not `STATUS_PENDING`.
    /// * `EscrowError::Unauthorized` - If `responder` does not match the swap's designated responder.
    /// * `EscrowError::HashLockMismatch` - If `SHA-256(secret) != hash_lock`.
    /// * `EscrowError::TimeLockExpired` - If current ledger timestamp `>= time_lock`.
    /// * `EscrowError::InsufficientBalance` - If `responder` has insufficient token balance.
    pub fn accept_swap(
        env: Env,
        id: u64,
        responder: Address,
        secret: BytesN<32>,
    ) -> Result<(), types::EscrowError> {
        contract::accept_swap(&env, id, &responder, secret)
    }

    /// Completes an active escrow swap and disburses tokens to both counterparties.
    ///
    /// Transfers `token_a` to `responder` and `token_b` to `initiator`.
    ///
    /// # Arguments
    /// * `env` - The contract environment.
    /// * `id` - Unique identifier of the active swap.
    /// * `caller` - Address initiating the completion (must be either initiator or responder).
    ///
    /// # Authorization
    /// * Requires authorization from `caller`.
    ///
    /// # Storage Touched
    /// * `DataKey::SwapRequests` (persistent): Updates swap status to `STATUS_COMPLETED`.
    ///
    /// # Events
    /// * Emits `SwapCompleted` event (`topics=["completed"]`) with `id`, `initiator`, `responder`.
    ///
    /// # Errors
    /// * `EscrowError::ContractPaused` - If the contract is paused.
    /// * `EscrowError::SwapNotFound` - If no swap with `id` exists.
    /// * `EscrowError::SwapNotActive` - If the swap status is not `STATUS_ACTIVE`.
    /// * `EscrowError::Unauthorized` - If `caller` is neither the initiator nor responder.
    pub fn complete_swap(env: Env, id: u64, caller: Address) -> Result<(), types::EscrowError> {
        contract::complete_swap(&env, id, &caller)
    }

    /// Cancels a swap and refunds locked funds to their respective depositors.
    ///
    /// If swap was `STATUS_PENDING`, refunds `token_a` to `initiator`.
    /// If swap was `STATUS_ACTIVE`, refunds `token_a` to `initiator` and `token_b` to `responder`.
    ///
    /// # Arguments
    /// * `env` - The contract environment.
    /// * `id` - Unique identifier of the swap.
    /// * `caller` - Address initiating cancellation (must be either initiator or responder).
    ///
    /// # Authorization
    /// * Requires authorization from `caller`.
    ///
    /// # Storage Touched
    /// * `DataKey::SwapRequests` (persistent): Updates swap status to `STATUS_CANCELLED`.
    ///
    /// # Events
    /// * Emits `SwapCancelled` event (`topics=["cancelled"]`) with `id`.
    ///
    /// # Errors
    /// * `EscrowError::ContractPaused` - If the contract is paused.
    /// * `EscrowError::SwapNotFound` - If no swap with `id` exists.
    /// * `EscrowError::SwapNotActive` - If the swap status is already `STATUS_COMPLETED` or `STATUS_CANCELLED`.
    /// * `EscrowError::Unauthorized` - If `caller` is neither the initiator nor responder.
    pub fn cancel_swap(env: Env, id: u64, caller: Address) -> Result<(), types::EscrowError> {
        contract::cancel_swap(&env, id, &caller)
    }

    /// Fetches the details of a specific swap request by ID.
    ///
    /// # Arguments
    /// * `env` - The contract environment.
    /// * `id` - Unique identifier of the swap.
    ///
    /// # Storage Touched
    /// * `DataKey::SwapRequests` (persistent): Reads swap record collection.
    ///
    /// # Returns
    /// * `Ok(SwapRequest)` - The full swap details.
    /// * `Err(EscrowError::SwapNotFound)` - If no swap with `id` exists.
    pub fn get_swap(env: Env, id: u64) -> Result<types::SwapRequest, types::EscrowError> {
        contract::get_swap(&env, id)
    }

    /// Fetches all swap requests recorded in contract storage.
    ///
    /// # Arguments
    /// * `env` - The contract environment.
    ///
    /// # Storage Touched
    /// * `DataKey::SwapRequests` (persistent): Reads swap record collection.
    ///
    /// # Returns
    /// * `Vec<SwapRequest>` - All stored swap requests, or empty vector if none exist.
    pub fn get_swaps(env: Env) -> soroban_sdk::Vec<types::SwapRequest> {
        contract::get_swaps(&env)
    }

    /// Pauses contract operations, preventing mutations (`create_swap`, `accept_swap`, `complete_swap`, `cancel_swap`).
    ///
    /// # Arguments
    /// * `env` - The contract environment.
    /// * `admin` - Administrator address.
    ///
    /// # Authorization
    /// * Requires authorization from `admin`.
    ///
    /// # Storage Touched
    /// * `DataKey::Admin` (instance): Reads administrator address.
    /// * Sets pause state in storage via common pause module.
    ///
    /// # Errors
    /// * `EscrowError::Unauthorized` - If `admin` does not match the configured contract administrator.
    /// * `EscrowError::ContractPaused` - If the contract is already paused.
    pub fn pause(env: Env, admin: Address) -> Result<(), types::EscrowError> {
        contract::pause(&env, &admin)
    }

    /// Unpauses contract operations, restoring normal swap execution.
    ///
    /// # Arguments
    /// * `env` - The contract environment.
    /// * `admin` - Administrator address.
    ///
    /// # Authorization
    /// * Requires authorization from `admin`.
    ///
    /// # Storage Touched
    /// * `DataKey::Admin` (instance): Reads administrator address.
    /// * Clears pause state in storage via common pause module.
    ///
    /// # Errors
    /// * `EscrowError::Unauthorized` - If `admin` does not match the configured contract administrator.
    /// * `EscrowError::ContractPaused` - If the contract is not paused.
    pub fn unpause(env: Env, admin: Address) -> Result<(), types::EscrowError> {
        contract::unpause(&env, &admin)
    }
}
