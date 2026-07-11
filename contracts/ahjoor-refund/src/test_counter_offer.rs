#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, String};
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

fn setup_counter_offer<'a>() -> (
    Env,
    AhjoorRefundContractClient<'a>,
    AhjoorPaymentsContractClient<'a>,
    Address, // admin
    Address, // customer
    Address, // merchant
    Address, // token_addr
    TokenClient<'a>,
    TokenAdminClient<'a>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let payment_id = env.register(AhjoorPaymentsContract, ());
    let payment_client = AhjoorPaymentsContractClient::new(&env, &payment_id);

    let refund_id = env.register(AhjoorRefundContract, ());
    let refund_client = AhjoorRefundContractClient::new(&env, &refund_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    payment_client.initialize(&admin, &admin, &0u32);
    payment_client.set_min_collateral(&0i128);
    payment_client.approve_merchant(&merchant);
    refund_client.initialize(&admin, &payment_id, &86_400u64, &None);

    // Fund customer and create a completed payment
    token_admin_client.mint(&customer, &2000);
    let pid = payment_client.create_payment(&customer, &merchant, &1000, &token_addr, &None, &None, &None);
    payment_client.complete_payment(&pid);

    // Fund refund contract so it can pay out
    token_admin_client.mint(&refund_id, &2000);

    // Customer requests refund
    refund_client.request_refund(
        &customer, &pid, &1000, &String::from_str(&env, "bad product"), &0u32,
    );

    (env, refund_client, payment_client, admin, customer, merchant, token_addr, token_client, token_admin_client)
}

// ---------------------------------------------------------------------------
// Test: accept path
// ---------------------------------------------------------------------------
#[test]
fn test_accept_counter_offer() {
    let (_env, refund_client, _, _admin, customer, merchant, _token_addr, token_client, _) = setup_counter_offer();
    let refund_id = 0u32;

    refund_client.counter_offer_refund(&merchant, &refund_id, &600);

    let refund = refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::CounterOffered);

    let bal_before = token_client.balance(&customer);
    refund_client.accept_counter_offer(&customer, &refund_id);

    let refund = refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert_eq!(token_client.balance(&customer), bal_before + 600);
}

// ---------------------------------------------------------------------------
// Test: reject path — escalates to admin (UnderAppeal)
// ---------------------------------------------------------------------------
#[test]
fn test_reject_counter_offer() {
    let (_env, refund_client, _, _admin, customer, merchant, _token_addr, _token_client, _) = setup_counter_offer();
    let refund_id = 0u32;

    refund_client.counter_offer_refund(&merchant, &refund_id, &600);
    refund_client.reject_counter_offer(&customer, &refund_id);

    let refund = refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::UnderAppeal);
}

// ---------------------------------------------------------------------------
// Test: expiry path — auto-escalates to admin
// ---------------------------------------------------------------------------
#[test]
fn test_counter_offer_expiry_escalates() {
    let (env, refund_client, _, admin, _customer, merchant, _token_addr, _token_client, _) = setup_counter_offer();
    let refund_id = 0u32;

    let expiry = 3600u64;
    refund_client.set_counter_offer_expiry_seconds(&admin, &expiry);
    refund_client.counter_offer_refund(&merchant, &refund_id, &600);

    // Advance past expiry
    env.ledger().with_mut(|l| l.timestamp += expiry + 1);

    refund_client.check_counter_offer_expiry(&refund_id);

    let refund = refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::UnderAppeal);
}

// ---------------------------------------------------------------------------
// Test: duplicate counter-offer rejected
// ---------------------------------------------------------------------------
#[test]
#[should_panic(expected = "Refund is not in Requested state")]
fn test_duplicate_counter_offer_rejected() {
    let (_env, refund_client, _, _admin, _customer, merchant, _token_addr, _token_client, _) = setup_counter_offer();
    let refund_id = 0u32;

    refund_client.counter_offer_refund(&merchant, &refund_id, &600);
    // Second counter-offer should panic
    refund_client.counter_offer_refund(&merchant, &refund_id, &400);
}

// ---------------------------------------------------------------------------
// Test: counter-offer amount cannot exceed original
// ---------------------------------------------------------------------------
#[test]
#[should_panic(expected = "Counter-offer cannot exceed original refund amount")]
fn test_counter_offer_exceeds_original() {
    let (_env, refund_client, _, _admin, _customer, merchant, _token_addr, _token_client, _) = setup_counter_offer();
    let refund_id = 0u32;

    refund_client.counter_offer_refund(&merchant, &refund_id, &9999);
}
