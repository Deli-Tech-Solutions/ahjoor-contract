#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

fn setup_proxy<'a>() -> (
    Env,
    AhjoorContractClient<'a>,
    Address,
    Address,
    TokenClient<'a>,
    TokenAdminClient<'a>,
    soroban_sdk::Vec<Address>,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..2 {
        let m = Address::generate(&env);
        token_admin_client.mint(&m, &10_000);
        members.push_back(m);
    }

    let proxy = Address::generate(&env);
    token_admin_client.mint(&proxy, &10_000);

    client.init(
        &admin,
        &members,
        &100,
        &token_addr,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
            late_fee_bps: 0,
            grace_period_seconds: 0,
            auction_enabled: false,
            auction_window_ledgers: 0,
            randomize_payout_order: false,
            reserve_enabled: false,
            reserve_contribution_bps: 0,
        },
        &None,
    );

    env.ledger().set_timestamp(100);

    (
        env,
        client,
        admin,
        token_addr,
        token_client,
        token_admin_client,
        members,
        proxy,
    )
}

#[test]
fn test_proxy_contribution_credits_member_debits_proxy() {
    let (_env, client, _admin, token_addr, token_client, _token_admin_client, members, proxy) =
        setup_proxy();

    let member = members.get(0).unwrap();

    let proxy_before = token_client.balance(&proxy);
    let member_before = token_client.balance(&member);

    client.authorize_proxy(&member, &0, &proxy, &2);
    client.contribute_as_proxy(&proxy, &0, &member, &token_addr, &100);

    let info = client.get_group_info();
    assert!(info.paid_members.contains(&member));
    assert_eq!(token_client.balance(&proxy), proxy_before - 100);
    assert_eq!(token_client.balance(&member), member_before);
}

#[test]
fn test_proxy_authorization_expires_after_max_rounds() {
    let (env, client, _admin, token_addr, _token_client, _token_admin_client, members, proxy) =
        setup_proxy();

    let member = members.get(0).unwrap();
    let other = members.get(1).unwrap();

    client.authorize_proxy(&member, &0, &proxy, &1);
    client.contribute_as_proxy(&proxy, &0, &member, &token_addr, &100);

    client.contribute(&other, &token_addr, &100);

    env.ledger().set_timestamp(200);

    let err = client
        .try_contribute_as_proxy(&proxy, &0, &member, &token_addr, &100)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, errors::ExtError::ProxyRoundsExhausted.into());
}

#[test]
fn test_proxy_round_limit_enforced() {
    let (env, client, _admin, token_addr, _token_client, token_admin_client, members, proxy) =
        setup_proxy();
    let member = members.get(0).unwrap();
    let other = members.get(1).unwrap();

    // Authorize for exactly 1 round
    client.authorize_proxy(&member, &0, &proxy, &1);

    // First contribution should succeed
    client.contribute_as_proxy(&proxy, &0, &member, &token_addr, &100);
    client.contribute(&other, &token_addr, &100);

    env.ledger().set_timestamp(200);

    // Second attempt must return ProxyRoundsExhausted, not NoDelegationFound
    let err = client
        .try_contribute_as_proxy(&proxy, &0, &member, &token_addr, &100)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, errors::ExtError::ProxyRoundsExhausted.into());
}

#[test]
fn test_revoke_proxy_removes_authorization() {
    let (_env, client, _admin, token_addr, _token_client, _token_admin_client, members, proxy) =
        setup_proxy();

    let member = members.get(0).unwrap();

    client.authorize_proxy(&member, &0, &proxy, &3);
    client.revoke_proxy(&member, &0, &proxy);

    let err = client
        .try_contribute_as_proxy(&proxy, &0, &member, &token_addr, &100)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, Error::NoDelegationFound.into());
}

#[test]
fn test_second_authorize_proxy_replaces_existing_proxy() {
    let (_env, client, _admin, token_addr, _token_client, token_admin_client, members, proxy1) =
        setup_proxy();

    let member = members.get(0).unwrap();
    let proxy2 = Address::generate(&_env);
    token_admin_client.mint(&proxy2, &10_000);

    client.authorize_proxy(&member, &0, &proxy1, &2);
    client.authorize_proxy(&member, &0, &proxy2, &2);

    let old_proxy_err = client
        .try_contribute_as_proxy(&proxy1, &0, &member, &token_addr, &100)
        .unwrap_err()
        .unwrap();
    assert_eq!(old_proxy_err, Error::NoDelegationFound.into());

    client.contribute_as_proxy(&proxy2, &0, &member, &token_addr, &100);
}

#[test]
fn test_proxy_wrong_amount_rejected() {
    let (_env, client, _admin, token_addr, _token_client, _token_admin_client, members, proxy) =
        setup_proxy();

    let member = members.get(0).unwrap();

    client.authorize_proxy(&member, &0, &proxy, &2);

    let err = client
        .try_contribute_as_proxy(&proxy, &0, &member, &token_addr, &90)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, errors::ExtError2::IncorrectContributionAmount.into());
}
