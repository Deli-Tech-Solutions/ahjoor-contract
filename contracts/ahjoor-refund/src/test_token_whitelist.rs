#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    Address, BytesN, Env, IntoVal, Map, String,
};

fn create_token_contract(e: &Env) -> Address {
    e.register_stellar_asset_contract(Address::generate(e))
}

fn create_funded_token(e: &Env, admin: &Address, customer: &Address, amount: i128) -> Address {
    let token_addr = e.register_stellar_asset_contract_v2(admin.clone()).address();
    soroban_sdk::token::StellarAssetClient::new(e, &token_addr).mint(customer, &amount);
    token_addr
}

fn create_whitelist_contract(e: &Env) -> Address {
    e.register(ahjoor_token_whitelist::TokenWhitelistContract, ())
}

fn create_refund_contract(e: &Env) -> Address {
    e.register_contract(None, AhjoorRefundContract)
}

fn create_payment_contract(e: &Env) -> Address {
    e.register(ahjoor_payments::AhjoorPaymentsContract, ())
}

#[test]
fn test_set_token_whitelist_contract() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let payment_contract = create_payment_contract(&e);
    let refund_contract = create_refund_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let client = AhjoorRefundContractClient::new(&e, &refund_contract);

    // Initialize refund contract
    client.initialize(&admin, &payment_contract, &86400u64, &None);

    // Set whitelist contract
    client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Verify it was set
    let stored_contract = client.get_token_whitelist_contract();
    assert_eq!(stored_contract, Some(whitelist_contract));
}

#[test]
fn test_token_validation_in_refund_request() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let customer = Address::generate(&e);
    let merchant = Address::generate(&e);
    // 1000 for payment + 500 for refund escrow
    let token = create_funded_token(&e, &admin, &customer, 1500);
    let payment_contract = create_payment_contract(&e);
    let refund_contract = create_refund_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let refund_client = AhjoorRefundContractClient::new(&e, &refund_contract);
    let payment_client = ahjoor_payments::AhjoorPaymentsContractClient::new(&e, &payment_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    // Initialize contracts
    refund_client.initialize(&admin, &payment_contract, &86400u64, &None);
    payment_client.initialize(&admin, &admin, &0u32);
    whitelist_client.initialize(&admin);

    // Set whitelist contract in refund
    refund_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Create a completed payment first
    let payment_id = payment_client.create_payment(
        &customer,
        &merchant,
        &1000i128,
        &token,
        &None::<String>,
        &None::<Map<String, String>>,
        &None::<BytesN<32>>,
    );

    // Complete the payment
    payment_client.complete_payment(&payment_id);

    // Try to request refund with non-whitelisted token - should fail
    let result = refund_client.try_request_refund(
        &customer,
        &payment_id,
        &500i128,
        &String::from_str(&e, "defective"),
        &0u32,
    );
    assert!(result.is_err());

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now refund request should succeed
    let refund_id = refund_client.request_refund(
        &customer,
        &payment_id,
        &500i128,
        &String::from_str(&e, "defective"),
        &0u32,
    );
    assert_eq!(refund_id, 0);
}

#[test]
fn test_token_validation_in_merchant_refund() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let customer = Address::generate(&e);
    let merchant = Address::generate(&e);
    let token = create_funded_token(&e, &admin, &customer, 1000);
    let payment_contract = create_payment_contract(&e);
    let refund_contract = create_refund_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let refund_client = AhjoorRefundContractClient::new(&e, &refund_contract);
    let payment_client = ahjoor_payments::AhjoorPaymentsContractClient::new(&e, &payment_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    // Initialize contracts
    refund_client.initialize(&admin, &payment_contract, &86400u64, &None);
    payment_client.initialize(&admin, &admin, &0u32);
    whitelist_client.initialize(&admin);

    // Set whitelist contract in refund
    refund_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Create a completed payment first
    let payment_id = payment_client.create_payment(
        &customer,
        &merchant,
        &1000i128,
        &token,
        &None::<String>,
        &None::<Map<String, String>>,
        &None::<BytesN<32>>,
    );

    // Complete the payment
    payment_client.complete_payment(&payment_id);

    // Try merchant refund with non-whitelisted token - should fail
    let result = refund_client.try_merchant_refund(&merchant, &payment_id, &500i128, &0u32);
    assert!(result.is_err());

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now merchant refund should succeed
    let refund_id = refund_client.merchant_refund(&merchant, &payment_id, &500i128, &0u32);
    assert_eq!(refund_id, 0);
}

#[test]
fn test_is_token_allowed_function() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let token = create_token_contract(&e);
    let payment_contract = create_payment_contract(&e);
    let refund_contract = create_refund_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let refund_client = AhjoorRefundContractClient::new(&e, &refund_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    // Initialize contracts
    refund_client.initialize(&admin, &payment_contract, &86400u64, &None);
    whitelist_client.initialize(&admin);

    // Without whitelist contract set, all tokens should be allowed
    assert!(refund_client.is_token_allowed(&token));

    // Set whitelist contract
    refund_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Token should not be allowed initially
    assert!(!refund_client.is_token_allowed(&token));

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now token should be allowed
    assert!(refund_client.is_token_allowed(&token));

    // Remove token from whitelist
    whitelist_client.remove_token(&admin, &token);

    // Token should not be allowed again
    assert!(!refund_client.is_token_allowed(&token));
}

#[test]
fn test_backward_compatibility_without_whitelist() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let customer = Address::generate(&e);
    let merchant = Address::generate(&e);
    // 1000 for payment + 500 for request_refund escrow
    let token = create_funded_token(&e, &admin, &customer, 1500);
    let payment_contract = create_payment_contract(&e);
    let refund_contract = create_refund_contract(&e);

    let refund_client = AhjoorRefundContractClient::new(&e, &refund_contract);
    let payment_client = ahjoor_payments::AhjoorPaymentsContractClient::new(&e, &payment_contract);

    // Initialize contracts without setting whitelist
    refund_client.initialize(&admin, &payment_contract, &86400u64, &None);
    payment_client.initialize(&admin, &admin, &0u32);

    // Create a completed payment
    let payment_id = payment_client.create_payment(
        &customer,
        &merchant,
        &1000i128,
        &token,
        &None::<String>,
        &None::<Map<String, String>>,
        &None::<BytesN<32>>,
    );
    payment_client.complete_payment(&payment_id);

    // Should be able to request refund with any token (backward compatibility)
    let refund_id = refund_client.request_refund(
        &customer,
        &payment_id,
        &500i128,
        &String::from_str(&e, "defective"),
        &0u32,
    );
    assert_eq!(refund_id, 0);

    // Should be able to do merchant refund with any token
    let refund_id2 = refund_client.merchant_refund(&merchant, &payment_id, &300i128, &0u32);
    assert_eq!(refund_id2, 1);
}

#[test]
fn test_only_admin_can_set_whitelist_contract() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let non_admin = Address::generate(&e);
    let payment_contract = create_payment_contract(&e);
    let refund_contract = create_refund_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let client = AhjoorRefundContractClient::new(&e, &refund_contract);

    // Initialize refund contract
    client.initialize(&admin, &payment_contract, &86400u64, &None);

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
    let payment_contract = create_payment_contract(&e);
    let refund_contract = create_refund_contract(&e);

    let client = AhjoorRefundContractClient::new(&e, &refund_contract);

    // Initialize refund contract
    client.initialize(&admin, &payment_contract, &86400u64, &None);

    // Should return None when no whitelist contract is set
    let stored_contract = client.get_token_whitelist_contract();
    assert_eq!(stored_contract, None);
}

#[test]
fn test_token_validation_with_multiple_tokens() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let customer = Address::generate(&e);
    let merchant = Address::generate(&e);
    // 1000 payment + 500 refund escrow for token1; 1000 payment only for token2 (request fails)
    let token1 = create_funded_token(&e, &admin, &customer, 1500);
    let token2 = create_funded_token(&e, &admin, &customer, 1000);
    let payment_contract = create_payment_contract(&e);
    let refund_contract = create_refund_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let refund_client = AhjoorRefundContractClient::new(&e, &refund_contract);
    let payment_client = ahjoor_payments::AhjoorPaymentsContractClient::new(&e, &payment_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    // Initialize contracts
    refund_client.initialize(&admin, &payment_contract, &86400u64, &None);
    payment_client.initialize(&admin, &admin, &0u32);
    whitelist_client.initialize(&admin);
    refund_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Add only token1 to whitelist
    whitelist_client.add_token(&admin, &token1);

    // token1 should be allowed
    assert!(refund_client.is_token_allowed(&token1));

    // token2 should not be allowed
    assert!(!refund_client.is_token_allowed(&token2));

    // Create payments with both tokens
    let payment_id1 = payment_client.create_payment(
        &customer,
        &merchant,
        &1000i128,
        &token1,
        &None::<String>,
        &None::<Map<String, String>>,
        &None::<BytesN<32>>,
    );
    payment_client.complete_payment(&payment_id1);

    let payment_id2 = payment_client.create_payment(
        &customer,
        &merchant,
        &1000i128,
        &token2,
        &None::<String>,
        &None::<Map<String, String>>,
        &None::<BytesN<32>>,
    );
    payment_client.complete_payment(&payment_id2);

    // Refund request with token1 should succeed
    let refund_id1 = refund_client.request_refund(
        &customer,
        &payment_id1,
        &500i128,
        &String::from_str(&e, "defective"),
        &0u32,
    );
    assert_eq!(refund_id1, 0);

    // Refund request with token2 should fail
    let result = refund_client.try_request_refund(
        &customer,
        &payment_id2,
        &500i128,
        &String::from_str(&e, "defective"),
        &0u32,
    );
    assert!(result.is_err());

    // Add token2 to whitelist
    whitelist_client.add_token(&admin, &token2);

    // Now both tokens should be allowed
    assert!(refund_client.is_token_allowed(&token1));
    assert!(refund_client.is_token_allowed(&token2));
}

#[test]
fn test_token_delisting_prevents_new_refunds() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let customer = Address::generate(&e);
    let merchant = Address::generate(&e);
    // 1000 + 1000 for two payments + 500 for request_refund escrow on payment_id1
    let token = create_funded_token(&e, &admin, &customer, 2500);
    let payment_contract = create_payment_contract(&e);
    let refund_contract = create_refund_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let refund_client = AhjoorRefundContractClient::new(&e, &refund_contract);
    let payment_client = ahjoor_payments::AhjoorPaymentsContractClient::new(&e, &payment_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    // Initialize contracts
    refund_client.initialize(&admin, &payment_contract, &86400u64, &None);
    payment_client.initialize(&admin, &admin, &0u32);
    whitelist_client.initialize(&admin);
    refund_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Create completed payments
    let payment_id1 = payment_client.create_payment(
        &customer,
        &merchant,
        &1000i128,
        &token,
        &None::<String>,
        &None::<Map<String, String>>,
        &None::<BytesN<32>>,
    );
    payment_client.complete_payment(&payment_id1);

    let payment_id2 = payment_client.create_payment(
        &customer,
        &merchant,
        &1000i128,
        &token,
        &None::<String>,
        &None::<Map<String, String>>,
        &None::<BytesN<32>>,
    );
    payment_client.complete_payment(&payment_id2);

    // Request refund successfully
    let refund_id = refund_client.request_refund(
        &customer,
        &payment_id1,
        &500i128,
        &String::from_str(&e, "defective"),
        &0u32,
    );
    assert_eq!(refund_id, 0);

    // Remove token from whitelist
    whitelist_client.remove_token(&admin, &token);

    // New refund request should fail
    let result = refund_client.try_request_refund(
        &customer,
        &payment_id2,
        &500i128,
        &String::from_str(&e, "defective"),
        &0u32,
    );
    assert!(result.is_err());
}
