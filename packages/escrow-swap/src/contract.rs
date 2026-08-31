use soroban_sdk::{token::Client as TokenClient, Address, BytesN, Env, Map, Vec};
use crate::types::*;
use common::pause;

pub fn init(env: &Env, admin: &Address) -> Result<(), EscrowError> {
    admin.require_auth();
    if env.storage().instance().has(&DataKey::Admin) {
        return Err(EscrowError::AlreadyInitialized);
    }
    if *admin == env.current_contract_address() {
        return Err(EscrowError::InvalidAdmin);
    }
    env.storage().instance().set(&DataKey::Admin, admin);
    env.storage().instance().set(&DataKey::NextSwapId, &0u64);
    env.storage().persistent().set(&DataKey::SwapRequests, &Map::<u64, SwapRequest>::new(env));
    Ok(())
}

/// Creates a new escrow swap between two parties.
pub fn create_swap(
    env: &Env,
    initiator: &Address,
    responder: &Address,
    token_a: &Address,
    token_b: &Address,
    initiator_amount: i128,
    responder_amount: i128,
    hash_lock: BytesN<32>,
    time_lock: u64,
) -> Result<u64, EscrowError> {
    pause::when_not_paused(env).map_err(|_| EscrowError::ContractPaused)?;
    let _guard = common::reentrancy::ReentrancyGuard::new(env).map_err(|_| EscrowError::NotInitialized)?;
    initiator.require_auth();

    if initiator == responder {
        return Err(EscrowError::InvalidSwap);
    }
    if initiator_amount <= 0 || responder_amount <= 0 {
        return Err(EscrowError::InvalidAmount);
    }

    let now = env.ledger().timestamp();
    if time_lock <= now {
        return Err(EscrowError::TimeLockExpired);
    }

    let next_id: u64 = env.storage().instance().get(&DataKey::NextSwapId).unwrap_or(0);
    let swap = SwapRequest {
        id: next_id,
        initiator: initiator.clone(),
        responder: responder.clone(),
        token_a: token_a.clone(),
        token_b: token_b.clone(),
        initiator_amount,
        responder_amount,
        hash_lock,
        time_lock,
        status: STATUS_PENDING,
        created_at: now,
    };

    let mut swaps: Map<u64, SwapRequest> = env.storage().persistent().get(&DataKey::SwapRequests).unwrap_or_else(|| Map::new(env));
    swaps.set(next_id, swap);
    env.storage().persistent().set(&DataKey::SwapRequests, &swaps);
    env.storage().instance().set(&DataKey::NextSwapId, &next_id.checked_add(1).ok_or(EscrowError::InvalidAmount)?);

    let token_a_client = TokenClient::new(env, token_a);
    token_a_client.transfer(initiator, &env.current_contract_address(), &initiator_amount);

    SwapCreated {
        id: next_id,
        initiator: initiator.clone(),
        responder: responder.clone(),
        token_a: token_a.clone(),
        token_b: token_b.clone(),
        initiator_amount,
        responder_amount,
    }
    .publish(env);

    Ok(next_id)
}

/// Accepts a pending swap by providing the secret that matches the hash lock.
pub fn accept_swap(env: &Env, id: u64, responder: &Address, secret: BytesN<32>) -> Result<(), EscrowError> {
    pause::when_not_paused(env).map_err(|_| EscrowError::ContractPaused)?;
    let _guard = common::reentrancy::ReentrancyGuard::new(env).map_err(|_| EscrowError::NotInitialized)?;
    responder.require_auth();

    let mut swaps: Map<u64, SwapRequest> = env.storage().persistent().get(&DataKey::SwapRequests).ok_or(EscrowError::NotInitialized)?;
    let mut s = swaps.get(id).ok_or(EscrowError::SwapNotFound)?;

    if s.status != STATUS_PENDING {
        return Err(EscrowError::SwapNotActive);
    }
    if s.responder != *responder {
        return Err(EscrowError::Unauthorized);
    }
    let secret_bytes = soroban_sdk::Bytes::from_array(env, &secret.to_array());
    let computed_hash: BytesN<32> = env.crypto().sha256(&secret_bytes).into();
    if computed_hash != s.hash_lock {
        return Err(EscrowError::HashLockMismatch);
    }
    let now = env.ledger().timestamp();
    if now >= s.time_lock {
        return Err(EscrowError::TimeLockExpired);
    }

    s.status = STATUS_ACTIVE;
    swaps.set(id, s.clone());

    let token_b_client = TokenClient::new(env, &s.token_b);
    token_b_client.transfer(responder, &env.current_contract_address(), &s.responder_amount);

    SwapAccepted { id, responder: responder.clone() }.publish(env);
    env.storage().persistent().set(&DataKey::SwapRequests, &swaps);
    Ok(())
}

pub fn complete_swap(env: &Env, id: u64, caller: &Address) -> Result<(), EscrowError> {
    pause::when_not_paused(env).map_err(|_| EscrowError::ContractPaused)?;
    let _guard = common::reentrancy::ReentrancyGuard::new(env).map_err(|_| EscrowError::NotInitialized)?;
    caller.require_auth();

    let mut swaps: Map<u64, SwapRequest> = env.storage().persistent().get(&DataKey::SwapRequests).ok_or(EscrowError::NotInitialized)?;
    let mut s = swaps.get(id).ok_or(EscrowError::SwapNotFound)?;

    if s.status != STATUS_ACTIVE {
        return Err(EscrowError::SwapNotActive);
    }
    if *caller != s.initiator && *caller != s.responder {
        return Err(EscrowError::Unauthorized);
    }
    s.status = STATUS_COMPLETED;
    swaps.set(id, s.clone());

    let token_a_client = TokenClient::new(env, &s.token_a);
    token_a_client.transfer(&env.current_contract_address(), &s.responder, &s.initiator_amount);

    let token_b_client = TokenClient::new(env, &s.token_b);
    token_b_client.transfer(&env.current_contract_address(), &s.initiator, &s.responder_amount);

    SwapCompleted { id, initiator: s.initiator.clone(), responder: s.responder.clone() }.publish(env);
    env.storage().persistent().set(&DataKey::SwapRequests, &swaps);
    Ok(())
}

pub fn cancel_swap(env: &Env, id: u64, caller: &Address) -> Result<(), EscrowError> {
    pause::when_not_paused(env).map_err(|_| EscrowError::ContractPaused)?;
    let _guard = common::reentrancy::ReentrancyGuard::new(env).map_err(|_| EscrowError::NotInitialized)?;
    caller.require_auth();

    let mut swaps: Map<u64, SwapRequest> = env.storage().persistent().get(&DataKey::SwapRequests).ok_or(EscrowError::NotInitialized)?;
    let mut s = swaps.get(id).ok_or(EscrowError::SwapNotFound)?;

    if s.status == STATUS_COMPLETED || s.status == STATUS_CANCELLED {
        return Err(EscrowError::SwapNotActive);
    }
    if *caller != s.initiator && *caller != s.responder {
        return Err(EscrowError::Unauthorized);
    }
    
    let previous_status = s.status;
    s.status = STATUS_CANCELLED;
    swaps.set(id, s.clone());

    let token_a_client = TokenClient::new(env, &s.token_a);
    token_a_client.transfer(&env.current_contract_address(), &s.initiator, &s.initiator_amount);

    if previous_status == STATUS_ACTIVE {
        let token_b_client = TokenClient::new(env, &s.token_b);
        token_b_client.transfer(&env.current_contract_address(), &s.responder, &s.responder_amount);
    }

    SwapCancelled { id }.publish(env);
    env.storage().persistent().set(&DataKey::SwapRequests, &swaps);
    Ok(())
}

pub fn get_swap(env: &Env, id: u64) -> Result<SwapRequest, EscrowError> {
    let swaps: Map<u64, SwapRequest> = env.storage().persistent().get(&DataKey::SwapRequests).ok_or(EscrowError::NotInitialized)?;
    swaps.get(id).ok_or(EscrowError::SwapNotFound)
}

pub fn get_swaps(env: &Env) -> Vec<SwapRequest> {
    let swaps: Map<u64, SwapRequest> = env.storage().persistent().get(&DataKey::SwapRequests).unwrap_or_else(|| Map::new(env));
    swaps.values()
}

pub fn pause(env: &Env, admin: &Address) -> Result<(), EscrowError> {
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(EscrowError::NotInitialized)?;
    if admin != &s {
        return Err(EscrowError::Unauthorized);
    }
    pause::pause(env, admin).map_err(|_| EscrowError::ContractPaused)
}

pub fn unpause(env: &Env, admin: &Address) -> Result<(), EscrowError> {
    let s: Address = env.storage().instance().get(&DataKey::Admin).ok_or(EscrowError::NotInitialized)?;
    if admin != &s {
        return Err(EscrowError::Unauthorized);
    }
    pause::unpause(env, admin).map_err(|_| EscrowError::ContractPaused)
}
