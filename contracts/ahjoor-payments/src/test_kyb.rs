#![cfg(test)]

use crate::{AhjoorPaymentsContract, AhjoorPaymentsContractClient, Error, ExtError};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, String,
};

fn create_token_contract<'a>(e: &Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
    let contract = e.register_stellar_asset_contract_v2(admin.clone());
    let contract_address = contract.address();
    let client = token::StellarAssetClient::new(e, &contract_address);
    (contract_address, client)
}

struct KybSetup<'a> {
    env: Env,
    admin: Address,
    customer: Address,
    merchant: Address,
    token: Address,
    client: AhjoorPaymentsContractClient<'a>,
}

fn setup() -> KybSetup<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let (token, token_client) = create_token_contract(&env, &token_admin);
    token_client.mint(&customer, &10_000);

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_recipient, &0);
    client.set_merchant_open_mode(&true);

    KybSetup {
        env,
        admin,
        customer,
        merchant,
        token,
        client,
    }
}

fn hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[test]
fn test_create_payment_requires_kyb_when_enabled() {
    let s = setup();
    s.client.set_kyb_enforcement(&s.admin, &true);

    let result = s.client.try_create_payment(
        &s.customer,
        &s.merchant,
        &1000,
        &s.token,
        &None,
        &None,
        &None,
    );

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().unwrap(),
        Error::KYBVerificationRequired.into()
    );
}

#[test]
fn test_create_payment_rejects_expired_kyb() {
    let s = setup();
    s.client.set_kyb_enforcement(&s.admin, &true);

    let now = u64::from(s.env.ledger().sequence());
    s.client.set_merchant_kyb(
        &s.admin,
        &s.merchant,
        &hash(&s.env, 1),
        &(now + 1),
        &String::from_str(&s.env, "NG"),
    );
    s.env.ledger().set_sequence_number((now + 2) as u32);

    let result = s.client.try_create_payment(
        &s.customer,
        &s.merchant,
        &1000,
        &s.token,
        &None,
        &None,
        &None,
    );

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), ExtError::MerchantKYBExpired.into());
}

#[test]
fn test_renew_merchant_kyb_allows_payment_again() {
    let s = setup();
    s.client.set_kyb_enforcement(&s.admin, &true);

    let now = u64::from(s.env.ledger().sequence());
    s.client.set_merchant_kyb(
        &s.admin,
        &s.merchant,
        &hash(&s.env, 2),
        &(now + 1),
        &String::from_str(&s.env, "GH"),
    );
    s.env.ledger().set_sequence_number((now + 2) as u32);

    let expired_result = s.client.try_create_payment(
        &s.customer,
        &s.merchant,
        &1000,
        &s.token,
        &None,
        &None,
        &None,
    );
    assert_eq!(
        expired_result.unwrap_err().unwrap(),
        ExtError::MerchantKYBExpired.into()
    );

    let renewed_expiry = u64::from(s.env.ledger().sequence()) + 1000;
    s.client.renew_merchant_kyb(
        &s.admin,
        &s.merchant,
        &hash(&s.env, 3),
        &renewed_expiry,
        &String::from_str(&s.env, "GH"),
    );

    let payment_id = s.client.create_payment(
        &s.customer,
        &s.merchant,
        &1000,
        &s.token,
        &None,
        &None,
        &None,
    );
    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.amount, 1000);
}

#[test]
fn test_create_payment_skips_kyb_when_not_required() {
    let s = setup();

    let payment_id = s.client.create_payment(
        &s.customer,
        &s.merchant,
        &1000,
        &s.token,
        &None,
        &None,
        &None,
    );
    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.amount, 1000);
}
