#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    Address, Env, IntoVal,
};

fn create_token_contract(e: &Env) -> Address {
    e.register_stellar_asset_contract(Address::generate(e))
}

fn create_funded_token(e: &Env, admin: &Address, buyer: &Address, amount: i128) -> Address {
    let token_addr = e.register_stellar_asset_contract_v2(admin.clone()).address();
    soroban_sdk::token::StellarAssetClient::new(e, &token_addr).mint(buyer, &amount);
    token_addr
}

fn create_whitelist_contract(e: &Env) -> Address {
    e.register(ahjoor_token_whitelist::TokenWhitelistContract, ())
}

fn create_escrow_contract(e: &Env) -> Address {
    e.register_contract(None, AhjoorEscrowContract)
}

#[test]
fn test_set_token_whitelist_contract() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let escrow_contract = create_escrow_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let client = AhjoorEscrowContractClient::new(&e, &escrow_contract);

    // Initialize escrow contract
    client.initialize(&admin);

    // Set whitelist contract
    client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Verify it was set
    let stored_contract = client.get_token_whitelist_contract();
    assert_eq!(stored_contract, Some(whitelist_contract));
}

#[test]
fn test_token_validation_in_escrow_creation() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let buyer = Address::generate(&e);
    let seller = Address::generate(&e);
    let arbiter = Address::generate(&e);
    let token = create_funded_token(&e, &admin, &buyer, 1000);
    let escrow_contract = create_escrow_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let escrow_client = AhjoorEscrowContractClient::new(&e, &escrow_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    // Initialize contracts
    escrow_client.initialize(&admin);
    whitelist_client.initialize(&admin);

    // Set whitelist contract in escrow
    escrow_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Try to create escrow with non-whitelisted token - should fail
    let result = escrow_client.try_create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000i128,
        &token,
        &(e.ledger().timestamp() + 3600),
        &None,
        &Vec::new(&e),
        &false,
        &0u32,
    );
    assert!(result.is_err());

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now escrow creation should succeed
    let escrow_id = escrow_client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000i128,
        &token,
        &(e.ledger().timestamp() + 3600),
        &None,
        &Vec::new(&e),
        &false,
        &0u32,
    );
    assert_eq!(escrow_id, 0);
}

#[test]
fn test_token_validation_in_insurance_config() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let token = create_token_contract(&e);
    let escrow_contract = create_escrow_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let escrow_client = AhjoorEscrowContractClient::new(&e, &escrow_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    // Initialize contracts
    escrow_client.initialize(&admin);
    whitelist_client.initialize(&admin);

    // Set whitelist contract in escrow
    escrow_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Try to set insurance config with non-whitelisted token - should fail
    let result = escrow_client.try_set_insurance_config(&admin, &token, &7u64);
    assert!(result.is_err());

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now insurance config should succeed
    escrow_client.set_insurance_config(&admin, &token, &7u64);
}

#[test]
fn test_is_token_allowed_function() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let token = create_token_contract(&e);
    let escrow_contract = create_escrow_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let escrow_client = AhjoorEscrowContractClient::new(&e, &escrow_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    // Initialize contracts
    escrow_client.initialize(&admin);
    whitelist_client.initialize(&admin);

    // Without whitelist contract set, all tokens should be allowed
    assert!(escrow_client.is_token_allowed(&token));

    // Set whitelist contract
    escrow_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Token should not be allowed initially
    assert!(!escrow_client.is_token_allowed(&token));

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now token should be allowed
    assert!(escrow_client.is_token_allowed(&token));

    // Remove token from whitelist
    whitelist_client.remove_token(&admin, &token);

    // Token should not be allowed again
    assert!(!escrow_client.is_token_allowed(&token));
}

#[test]
fn test_backward_compatibility_without_whitelist() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let buyer = Address::generate(&e);
    let seller = Address::generate(&e);
    let arbiter = Address::generate(&e);
    let token_addr = e.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token_addr);
    token_admin.mint(&buyer, &1000);
    let escrow_contract = create_escrow_contract(&e);

    let escrow_client = AhjoorEscrowContractClient::new(&e, &escrow_contract);

    // Initialize escrow contract without setting whitelist
    escrow_client.initialize(&admin);

    // Should be able to create escrow with any token (backward compatibility)
    let escrow_id = escrow_client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000i128,
        &token_addr,
        &(e.ledger().timestamp() + 3600),
        &None,
        &Vec::new(&e),
        &false,
        &0u32,
    );
    assert_eq!(escrow_id, 0);

    // Should be able to set insurance config with any token
    escrow_client.set_insurance_config(&admin, &token_addr, &7u64);
}

#[test]
fn test_only_admin_can_set_whitelist_contract() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let non_admin = Address::generate(&e);
    let escrow_contract = create_escrow_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let client = AhjoorEscrowContractClient::new(&e, &escrow_contract);

    // Initialize escrow contract
    client.initialize(&admin);

    // Non-admin should not be able to set whitelist contract
    let result = client.try_set_token_whitelist_contract(&non_admin, &whitelist_contract);
    assert!(result.is_err());

    // Admin should be able to set whitelist contract
    client.set_token_whitelist_contract(&admin, &whitelist_contract);
    let stored_contract = client.get_token_whitelist_contract();
    assert_eq!(stored_contract, Some(whitelist_contract));
}

#[test]
fn test_get_token_whitelist_contract_when_not_set() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let escrow_contract = create_escrow_contract(&e);

    let client = AhjoorEscrowContractClient::new(&e, &escrow_contract);

    // Initialize escrow contract
    client.initialize(&admin);

    // Should return None when no whitelist contract is set
    let stored_contract = client.get_token_whitelist_contract();
    assert_eq!(stored_contract, None);
}

#[test]
fn test_token_validation_with_multiple_tokens() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let buyer = Address::generate(&e);
    let seller = Address::generate(&e);
    let arbiter = Address::generate(&e);
    let token1 = create_funded_token(&e, &admin, &buyer, 1000);
    let token2 = create_token_contract(&e);
    let escrow_contract = create_escrow_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let escrow_client = AhjoorEscrowContractClient::new(&e, &escrow_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    // Initialize contracts
    escrow_client.initialize(&admin);
    whitelist_client.initialize(&admin);
    escrow_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Add only token1 to whitelist
    whitelist_client.add_token(&admin, &token1);

    // token1 should be allowed
    assert!(escrow_client.is_token_allowed(&token1));

    // token2 should not be allowed
    assert!(!escrow_client.is_token_allowed(&token2));

    // Escrow creation with token1 should succeed
    let escrow_id1 = escrow_client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000i128,
        &token1,
        &(e.ledger().timestamp() + 3600),
        &None,
        &Vec::new(&e),
        &false,
        &0u32,
    );
    assert_eq!(escrow_id1, 0);

    // Escrow creation with token2 should fail
    let result = escrow_client.try_create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000i128,
        &token2,
        &(e.ledger().timestamp() + 3600),
        &None,
        &Vec::new(&e),
        &false,
        &0u32,
    );
    assert!(result.is_err());

    // Add token2 to whitelist
    whitelist_client.add_token(&admin, &token2);

    // Now both tokens should be allowed
    assert!(escrow_client.is_token_allowed(&token1));
    assert!(escrow_client.is_token_allowed(&token2));
}

#[test]
fn test_token_delisting_prevents_new_escrows() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let buyer = Address::generate(&e);
    let seller = Address::generate(&e);
    let arbiter = Address::generate(&e);
    let token = create_funded_token(&e, &admin, &buyer, 1000);
    let escrow_contract = create_escrow_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let escrow_client = AhjoorEscrowContractClient::new(&e, &escrow_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    // Initialize contracts
    escrow_client.initialize(&admin);
    whitelist_client.initialize(&admin);
    escrow_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Create escrow successfully
    let escrow_id = escrow_client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000i128,
        &token,
        &(e.ledger().timestamp() + 3600),
        &None,
        &Vec::new(&e),
        &false,
        &0u32,
    );
    assert_eq!(escrow_id, 0);

    // Remove token from whitelist
    whitelist_client.remove_token(&admin, &token);

    // New escrow creation should fail
    let result = escrow_client.try_create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000i128,
        &token,
        &(e.ledger().timestamp() + 3600),
        &None,
        &Vec::new(&e),
        &false,
        &0u32,
    );
    assert!(result.is_err());
}