#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::Address as _,
    Address, Env, String,
};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TokenAdminClient};
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

fn setup_cc<'a>() -> (
    Env,
    AhjoorRefundContractClient<'a>,
    AhjoorPaymentsContractClient<'a>,
    Address, // admin
    Address, // token
    TokenAdminClient<'a>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let payment_id = env.register(AhjoorPaymentsContract, ());
    let payment_client = AhjoorPaymentsContractClient::new(&env, &payment_id);

    let refund_id = env.register(AhjoorRefundContract, ());
    let refund_client = AhjoorRefundContractClient::new(&env, &refund_id);

    let admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_admin = TokenAdminClient::new(&env, &token_addr);

    payment_client.initialize(&admin, &admin, &0u32);
    refund_client.initialize(&admin, &payment_id, &86_400u64, &None);

    (env, refund_client, payment_client, admin, token_addr, token_admin)
}

#[test]
fn test_register_cross_contract_refund_success() {
    let (env, refund_client, _payment_client, admin, token_addr, _token_admin) = setup_cc();

    let origin_contract = Address::generate(&env);
    refund_client.add_refund_origin_contract(&admin, &origin_contract);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let rid = refund_client.register_cross_contract_refund(
        &origin_contract,
        &1u32,
        &customer,
        &merchant,
        &token_addr,
        &500i128,
        &1u32,
    );

    let refund = refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::CrossContractRefunded);
    assert_eq!(refund.origin_contract, Some(origin_contract));
    assert_eq!(refund.escrow_id, Some(1u32));
    assert_eq!(refund.auto_approved_source, Some(String::from_str(&env, "cross_contract")));
}

#[test]
#[should_panic(expected = "UnauthorisedOriginContract")]
fn test_non_whitelisted_caller_rejected() {
    let (env, refund_client, _payment_client, _admin, token_addr, _token_admin) = setup_cc();

    let rogue = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    refund_client.register_cross_contract_refund(
        &rogue,
        &1u32,
        &customer,
        &merchant,
        &token_addr,
        &500i128,
        &1u32,
    );
}

#[test]
fn test_cross_contract_refund_appears_in_queue() {
    let (env, refund_client, _payment_client, admin, token_addr, _token_admin) = setup_cc();

    let origin = Address::generate(&env);
    refund_client.add_refund_origin_contract(&admin, &origin);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let rid = refund_client.register_cross_contract_refund(
        &origin, &10u32, &customer, &merchant, &token_addr, &200i128, &0u32,
    );

    let (items, total, _) = refund_client.get_cross_contract_refund_queue(&0u32, &50u32);
    assert_eq!(total, 1);
    assert_eq!(items.get(0).unwrap().id, rid);
}

#[test]
fn test_cross_contract_refund_counted_in_merchant_metrics() {
    let (env, refund_client, _payment_client, admin, token_addr, _token_admin) = setup_cc();

    let origin = Address::generate(&env);
    refund_client.add_refund_origin_contract(&admin, &origin);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    refund_client.register_cross_contract_refund(
        &origin, &5u32, &customer, &merchant, &token_addr, &300i128, &0u32,
    );

    let stats = refund_client.get_merchant_refund_stats(&merchant);
    assert_eq!(stats.total_requested, 1);
    assert_eq!(stats.total_processed, 1);
    assert_eq!(stats.total_amount_refunded, 300);
}

#[test]
fn test_add_and_remove_origin_contract() {
    let (env, refund_client, _payment_client, admin, _token_addr, _token_admin) = setup_cc();

    let origin = Address::generate(&env);
    refund_client.add_refund_origin_contract(&admin, &origin);

    let whitelist = refund_client.get_cross_contract_whitelist();
    assert_eq!(whitelist.len(), 1);

    refund_client.remove_refund_origin_contract(&admin, &origin);
    let whitelist = refund_client.get_cross_contract_whitelist();
    assert_eq!(whitelist.len(), 0);
}

#[test]
#[should_panic(expected = "ContractAlreadyWhitelisted")]
fn test_cannot_whitelist_same_contract_twice() {
    let (env, refund_client, _payment_client, admin, _token_addr, _token_admin) = setup_cc();

    let origin = Address::generate(&env);
    refund_client.add_refund_origin_contract(&admin, &origin);
    refund_client.add_refund_origin_contract(&admin, &origin);
}
