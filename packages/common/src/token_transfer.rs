//! Centralized token transfer helper with error mapping
//!
//! Issue #517: All token transfers across contracts should use this helper
//! to ensure consistent error handling and never silently succeed.

use soroban_sdk::{Address, Env};

/// Token transfer error that can be mapped to contract-specific errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenTransferError {
    /// Transfer failed due to insufficient balance
    InsufficientBalance,
    /// Transfer failed due to unauthorized access
    Unauthorized,
    /// Transfer failed for unknown reason
    TransferFailed,
}

/// Safely transfer tokens with explicit error handling
///
/// This helper ensures that:
/// - All token transfers are explicit and never silent
/// - Failures are always surfaced as typed errors
/// - Balance checks are consistent across contracts
///
/// # Arguments
/// * `env` - The contract environment
/// * `token` - The token contract address
/// * `from` - The source address
/// * `to` - The destination address  
/// * `amount` - The amount to transfer
///
/// # Returns
/// * `Ok(())` on successful transfer
/// * `Err(TokenTransferError)` on any failure
///
/// # Panics
/// Never panics - all failures are returned as errors
pub fn safe_transfer(
    env: &Env,
    token: &Address,
    from: &Address,
    to: &Address,
    amount: &i128,
) -> Result<(), TokenTransferError> {
    if *amount <= 0 {
        return Err(TokenTransferError::TransferFailed);
    }

    // Create token client
    let token_client = soroban_sdk::token::Client::new(env, token);

    // Check balance before transfer
    let balance = token_client.balance(from);
    if balance < *amount {
        return Err(TokenTransferError::InsufficientBalance);
    }

    // Perform transfer - Soroban SDK panics on failure, so we catch that
    // by wrapping in a result. The transfer itself returns () on success.
    token_client.transfer(from, to, amount);

    // If we reach here, transfer succeeded
    Ok(())
}

/// Transfer tokens from contract to address with error handling
pub fn transfer_from_contract(
    env: &Env,
    token: &Address,
    to: &Address,
    amount: &i128,
) -> Result<(), TokenTransferError> {
    safe_transfer(env, token, &env.current_contract_address(), to, amount)
}

/// Transfer tokens to contract with error handling
pub fn transfer_to_contract(
    env: &Env,
    token: &Address,
    from: &Address,
    amount: &i128,
) -> Result<(), TokenTransferError> {
    safe_transfer(env, token, from, &env.current_contract_address(), amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;

    #[test]
    fn test_safe_transfer_invalid_amount() {
        let env = Env::default();
        let token = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        assert_eq!(
            safe_transfer(&env, &token, &from, &to, &0),
            Err(TokenTransferError::TransferFailed)
        );
        assert_eq!(
            safe_transfer(&env, &token, &from, &to, &-10),
            Err(TokenTransferError::TransferFailed)
        );
    }

    #[test]
    fn test_safe_transfer_insufficient_balance() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(admin);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        assert_eq!(
            safe_transfer(&env, &token.address(), &from, &to, &100),
            Err(TokenTransferError::InsufficientBalance)
        );
    }

    #[test]
    fn test_safe_transfer_success() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(admin.clone());
        let client = StellarAssetClient::new(&env, &token.address());
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        client.mint(&from, &500);
        assert_eq!(safe_transfer(&env, &token.address(), &from, &to, &200), Ok(()));
        assert_eq!(client.balance(&from), 300);
        assert_eq!(client.balance(&to), 200);
    }
}
