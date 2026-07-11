#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};

fn setup<'a>() -> (Env, AhjoorEscrowContractClient<'a>, Address, Address, TokenClient<'a>, TokenAdminClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin);
    client.add_allowed_token(&admin, &token_addr);

    (env, client, admin, token_addr, token_client, token_admin_client)
}

#[test]
fn test_create_multi_buyer_escrow_and_unanimous_release() {
    let (env, client, _admin, token_addr, token_client, token_admin_client) = setup();
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    token_admin_client.mint(&buyer1, &1_000);
    token_admin_client.mint(&buyer2, &1_000);
    token_client.approve(&buyer1, &client.address, &300, &(env.ledger().sequence() + 10_000));
    token_client.approve(&buyer2, &client.address, &700, &(env.ledger().sequence() + 10_000));

    let mut buyers: Vec<(Address, i128)> = Vec::new(&env);
    buyers.push_back((buyer1.clone(), 300));
    buyers.push_back((buyer2.clone(), 700));

    let deadline = env.ledger().timestamp() + 1_000;
    let escrow_id = client.create_multi_buyer_escrow(
        &buyers,
        &seller,
        &arbiter,
        &token_addr,
        &deadline,
    );

    // First buyer approval only records approval; should not release yet.
    client.release_escrow(&buyer1, &escrow_id);
    let mid = client.get_escrow(&escrow_id);
    assert_eq!(mid.status, EscrowStatus::Active);
    assert_eq!(token_client.balance(&seller), 0);

    // Final buyer approval triggers release.
    client.release_escrow(&buyer2, &escrow_id);
    let end = client.get_escrow(&escrow_id);
    assert_eq!(end.status, EscrowStatus::Released);
    assert_eq!(token_client.balance(&seller), 1_000);
}

#[test]
fn test_multi_buyer_refund_proportional_on_expiry() {
    let (env, client, _admin, token_addr, token_client, token_admin_client) = setup();
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    token_admin_client.mint(&buyer1, &1_000);
    token_admin_client.mint(&buyer2, &1_000);
    token_client.approve(&buyer1, &client.address, &200, &(env.ledger().sequence() + 10_000));
    token_client.approve(&buyer2, &client.address, &800, &(env.ledger().sequence() + 10_000));

    let mut buyers: Vec<(Address, i128)> = Vec::new(&env);
    buyers.push_back((buyer1.clone(), 200));
    buyers.push_back((buyer2.clone(), 800));

    let deadline = env.ledger().timestamp() + 10;
    let escrow_id = client.create_multi_buyer_escrow(
        &buyers,
        &seller,
        &arbiter,
        &token_addr,
        &deadline,
    );

    // Move past deadline and auto-refund.
    env.ledger().set_timestamp(deadline + 1);
    client.auto_release_expired(&escrow_id);

    // Contributions are refunded in original proportions.
    assert_eq!(token_client.balance(&buyer1), 1_000);
    assert_eq!(token_client.balance(&buyer2), 1_000);
}
