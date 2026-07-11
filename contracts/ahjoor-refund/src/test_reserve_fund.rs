#![cfg(test)]
extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env};
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;

use crate::{AhjoorRefundContract, AhjoorRefundContractClient, RefundInitConfig};

fn setup(env: &Env) -> (AhjoorRefundContractClient<'static>, Address, Address) {
    let contract_id = env.register(AhjoorRefundContract, ());
    let client = AhjoorRefundContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let payment_contract = Address::generate(env); // mock
    client.initialize(
        &admin,
        &payment_contract,
        &86400u64,
        &None::<RefundInitConfig>,
    );
    (client, admin, payment_contract)
}

fn make_token<'a>(env: &'a Env, admin: &Address) -> (Address, TokenAdminClient<'a>) {
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_admin = TokenAdminClient::new(env, &token_addr);
    (token_addr, token_admin)
}

#[test]
fn test_deposit_and_withdraw_reserve() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let merchant = Address::generate(&env);
    let (token_addr, token_admin) = make_token(&env, &admin);

    token_admin.mint(&merchant, &2000i128);

    // Set ratio
    client.set_reserve_ratio_bps(&admin, &200u32);

    // Deposit
    client.deposit_reserve(&merchant, &token_addr, &1000i128);
    assert_eq!(client.get_merchant_reserve(&merchant), 1000i128);

    // Withdraw within allowed amount (no volume recorded, required = 0)
    client.withdraw_reserve(&merchant, &token_addr, &500i128);
    assert_eq!(client.get_merchant_reserve(&merchant), 500i128);
}

#[test]
#[should_panic(expected = "WithdrawalWouldBreachMinimum")]
fn test_withdraw_below_minimum_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let merchant = Address::generate(&env);
    let (token_addr, token_admin) = make_token(&env, &admin);

    token_admin.mint(&merchant, &2000i128);

    client.set_reserve_ratio_bps(&admin, &200u32);
    // Deposit 100 tokens
    client.deposit_reserve(&merchant, &token_addr, &100i128);
    // Record volume=10000 → required=200, but balance=100 → already below required
    // Attempting to withdraw 50 more should breach minimum
    client.record_payment_volume(&merchant, &10_000i128);
    client.withdraw_reserve(&merchant, &token_addr, &50i128);
}

#[test]
fn test_compliance_check_flags_merchant() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let merchant = Address::generate(&env);

    client.set_reserve_ratio_bps(&admin, &200u32);
    // Volume = 10000, required = 200, reserve = 0 → non-compliant
    client.record_payment_volume(&merchant, &10_000i128);
    let compliant = client.check_reserve_compliance(&admin, &merchant);
    assert!(!compliant);
}

#[test]
fn test_compliance_check_passes_when_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let merchant = Address::generate(&env);

    client.set_reserve_ratio_bps(&admin, &200u32);
    // No volume → required = 0 → always compliant
    let compliant = client.check_reserve_compliance(&admin, &merchant);
    assert!(compliant);
}
