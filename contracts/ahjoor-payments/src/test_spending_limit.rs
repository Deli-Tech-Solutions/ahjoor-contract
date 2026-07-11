#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

fn setup_spend<'a>() -> (Env, AhjoorPaymentsContractClient<'a>, Address, Address, Address, TokenClient<'a>, TokenAdminClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin, &admin, &0u32);
    client.set_min_collateral(&0i128);
    client.approve_merchant(&merchant);

    (env, client, admin, merchant, token_addr, token_client, token_admin_client)
}

// ---------------------------------------------------------------------------
// Test: payment within limit succeeds
// ---------------------------------------------------------------------------
#[test]
fn test_within_limit_succeeds() {
    let (env, client, _admin, merchant, token_addr, _tc, tac) = setup_spend();
    let customer = Address::generate(&env);
    tac.mint(&customer, &1000);

    client.set_customer_spend_limit(&merchant, &customer, &500, &3600u64);

    let pid = client.create_payment(&customer, &merchant, &300, &token_addr, &None, &None, &None);
    client.complete_payment(&pid); // should succeed
    assert_eq!(client.get_payment(&pid).status, PaymentStatus::Completed);
}

// ---------------------------------------------------------------------------
// Test: payment exceeding limit is rejected
// ---------------------------------------------------------------------------
#[test]
fn test_limit_exceeded_rejected() {
    let (env, client, _admin, merchant, token_addr, _tc, tac) = setup_spend();
    let customer = Address::generate(&env);
    tac.mint(&customer, &1000);

    client.set_customer_spend_limit(&merchant, &customer, &200, &3600u64);

    let pid = client.create_payment(&customer, &merchant, &300, &token_addr, &None, &None, &None);
    let result = client.try_complete_payment(&pid);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: window resets after expiry
// ---------------------------------------------------------------------------
#[test]
fn test_window_reset() {
    let (env, client, _admin, merchant, token_addr, _tc, tac) = setup_spend();
    let customer = Address::generate(&env);
    tac.mint(&customer, &1000);

    client.set_customer_spend_limit(&merchant, &customer, &200, &3600u64);

    let pid1 = client.create_payment(&customer, &merchant, &200, &token_addr, &None, &None, &None);
    client.complete_payment(&pid1);

    // Advance time past the window
    env.ledger().with_mut(|l| l.timestamp += 3601);

    // Now a new payment within the limit should succeed
    let pid2 = client.create_payment(&customer, &merchant, &200, &token_addr, &None, &None, &None);
    client.complete_payment(&pid2);
    assert_eq!(client.get_payment(&pid2).status, PaymentStatus::Completed);
}

// ---------------------------------------------------------------------------
// Test: individual override takes priority over default
// ---------------------------------------------------------------------------
#[test]
fn test_individual_override_priority() {
    let (env, client, _admin, merchant, token_addr, _tc, tac) = setup_spend();
    let customer = Address::generate(&env);
    tac.mint(&customer, &1000);

    // Default is very low
    client.set_default_spend_limit(&merchant, &50, &3600u64);
    // Individual override is higher
    client.set_customer_spend_limit(&merchant, &customer, &500, &3600u64);

    let pid = client.create_payment(&customer, &merchant, &300, &token_addr, &None, &None, &None);
    client.complete_payment(&pid); // should succeed using individual limit
    assert_eq!(client.get_payment(&pid).status, PaymentStatus::Completed);
}

// ---------------------------------------------------------------------------
// Test: removing limit restores normal flow
// ---------------------------------------------------------------------------
#[test]
fn test_limit_removal_restores_flow() {
    let (env, client, _admin, merchant, token_addr, _tc, tac) = setup_spend();
    let customer = Address::generate(&env);
    tac.mint(&customer, &1000);

    client.set_customer_spend_limit(&merchant, &customer, &100, &3600u64);

    // First payment hits the limit
    let pid1 = client.create_payment(&customer, &merchant, &100, &token_addr, &None, &None, &None);
    client.complete_payment(&pid1);

    // Second payment would exceed — remove limit first
    client.remove_customer_spend_limit(&merchant, &customer);

    let pid2 = client.create_payment(&customer, &merchant, &500, &token_addr, &None, &None, &None);
    client.complete_payment(&pid2);
    assert_eq!(client.get_payment(&pid2).status, PaymentStatus::Completed);
}

// ---------------------------------------------------------------------------
// Test: default limit applies to customers without individual override
// ---------------------------------------------------------------------------
#[test]
fn test_default_limit_applies() {
    let (env, client, _admin, merchant, token_addr, _tc, tac) = setup_spend();
    let customer = Address::generate(&env);
    tac.mint(&customer, &1000);

    client.set_default_spend_limit(&merchant, &100, &3600u64);

    let pid = client.create_payment(&customer, &merchant, &200, &token_addr, &None, &None, &None);
    let result = client.try_complete_payment(&pid);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: window resets on fixed boundary, not on last-payment time
// A customer making one small payment every window_seconds - 1 seconds must
// still hit the cap within the fixed window.
// ---------------------------------------------------------------------------
#[test]
fn test_window_resets_on_boundary() {
    let (env, client, _admin, merchant, token_addr, _tc, tac) = setup_spend();
    let customer = Address::generate(&env);
    tac.mint(&customer, &5000);

    let cap: i128 = 300;
    let window: u64 = 3600;
    client.set_customer_spend_limit(&merchant, &customer, &cap, &window);

    // Payment 1 at t=0: spent = 100, now window_start = 0
    env.ledger().with_mut(|l| l.timestamp = 0);
    let pid1 = client.create_payment(&customer, &merchant, &100, &token_addr, &None, &None, &None);
    client.complete_payment(&pid1);

    // Payment 2 at t=3599 (just inside window): spent = 200
    env.ledger().with_mut(|l| l.timestamp = 3599);
    let pid2 = client.create_payment(&customer, &merchant, &100, &token_addr, &None, &None, &None);
    client.complete_payment(&pid2);

    // Payment 3 at t=3599 (still inside same window): would push spent=300, within cap (300)
    let pid3 = client.create_payment(&customer, &merchant, &100, &token_addr, &None, &None, &None);
    client.complete_payment(&pid3);

    // Payment 4 at t=3599 (still inside same window): would push spent=400 > 300, rejected
    let pid4 = client.create_payment(&customer, &merchant, &100, &token_addr, &None, &None, &None);
    let result = client.try_complete_payment(&pid4);
    assert!(result.is_err(), "Fourth payment should exceed the cap within the window");

    // Jump past the window boundary: now >= window_start + window_seconds
    // window_start = 0, window_seconds = 3600, so at t=3600 the window resets
    env.ledger().with_mut(|l| l.timestamp = 3600);

    // Payment 5 at t=3600: window resets, spent = 0 + 100 = 100
    let pid5 = client.create_payment(&customer, &merchant, &100, &token_addr, &None, &None, &None);
    client.complete_payment(&pid5);

    // Verify we can use the full cap again in the new window
    let pid6 = client.create_payment(&customer, &merchant, &200, &token_addr, &None, &None, &None);
    client.complete_payment(&pid6);

    // Further payments in this window should be rejected
    let pid7 = client.create_payment(&customer, &merchant, &100, &token_addr, &None, &None, &None);
    let result = client.try_complete_payment(&pid7);
    assert!(result.is_err(), "New window cap should still be enforced");
}
