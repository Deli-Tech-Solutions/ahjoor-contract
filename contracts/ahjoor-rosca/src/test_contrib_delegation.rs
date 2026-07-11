#![cfg(test)]
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, Vec};
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::token::Client as TokenClient;

use crate::{AhjoorContract, AhjoorContractClient, RoscaConfig, PayoutStrategy, VotingMode};

fn make_config(env: &Env) -> RoscaConfig {
    RoscaConfig {
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
        max_members: Some(10),
        skip_fee: 0,
        max_skips_per_cycle: 5,
        voting_mode: VotingMode::Equal,
        late_fee_bps: 0,
        grace_period_seconds: 0,
        auction_enabled: false,
        auction_window_ledgers: 0,
        randomize_payout_order: false,
        reserve_enabled: false,
        reserve_contribution_bps: 0,
    }
}

fn setup_rosca<'a>(
    env: &'a Env,
    members: &[Address],
) -> (AhjoorContractClient<'a>, Address, Address, Address) {
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let token_admin = TokenAdminClient::new(env, &token_addr);
    let token_client = TokenClient::new(env, &token_addr);

    let members_vec: Vec<Address> = Vec::from_slice(env, members);

    // Mint tokens to each member
    for m in members.iter() {
        token_admin.mint(m, &100_000);
        token_client.approve(m, &contract_id, &100_000, &(env.ledger().sequence() + 10_000));
    }

    let config = make_config(env);
    client.init(&admin, &members_vec, &1_000i128, &token_addr, &1000u64, &config, &None);

    (client, admin, token_addr, contract_id)
}

// ── Delegation tests ──────────────────────────────────────────────────────────

#[test]
fn test_delegate_and_get_delegation() {
    let env = Env::default();
    let member = Address::generate(&env);
    let proxy = Address::generate(&env);
    let (client, _admin, _token, _) = setup_rosca(&env, &[member.clone()]);

    let expiry = (env.ledger().sequence() as u64) + 1_000;
    client.delegate_contribution_rights(&member, &0u32, &proxy, &expiry);

    let rec = client.get_member_delegation(&0u32, &member).unwrap();
    assert_eq!(rec.proxy, proxy);
    assert_eq!(rec.expiry, expiry);
}

#[test]
fn test_new_delegation_replaces_old_proxy() {
    let env = Env::default();
    let member = Address::generate(&env);
    let proxy_a = Address::generate(&env);
    let proxy_b = Address::generate(&env);
    let (client, _admin, _token, _) = setup_rosca(&env, &[member.clone()]);

    let expiry = (env.ledger().sequence() as u64) + 1_000;
    client.delegate_contribution_rights(&member, &0u32, &proxy_a, &expiry);
    client.delegate_contribution_rights(&member, &0u32, &proxy_b, &expiry);

    let rec = client.get_member_delegation(&0u32, &member).unwrap();
    assert_eq!(rec.proxy, proxy_b);
}

#[test]
fn test_revoke_delegation_clears_record() {
    let env = Env::default();
    let member = Address::generate(&env);
    let proxy = Address::generate(&env);
    let (client, _admin, _token, _) = setup_rosca(&env, &[member.clone()]);

    let expiry = (env.ledger().sequence() as u64) + 1_000;
    client.delegate_contribution_rights(&member, &0u32, &proxy, &expiry);
    client.revoke_contribution_delegation(&member, &0u32);

    assert!(client.get_member_delegation(&0u32, &member).is_none());
}

#[test]
fn test_proxy_can_contribute_on_behalf() {
    let env = Env::default();
    let member = Address::generate(&env);
    let proxy = Address::generate(&env);
    let (client, _admin, token, contract_id) = setup_rosca(&env, &[member.clone()]);

    let token_admin = TokenAdminClient::new(&env, &token);
    let token_client = TokenClient::new(&env, &token);

    // Give proxy tokens and approve contract
    token_admin.mint(&proxy, &5_000);
    token_client.approve(&proxy, &contract_id, &5_000, &(env.ledger().sequence() + 10_000));

    // Also need member to approve proxy as a spender (for transfer_from proxy → member → contract)
    // In our implementation proxy pays from member's account via transfer_from(proxy, member, ...)
    // member must have approved proxy to spend
    token_client.approve(&member, &proxy, &5_000, &(env.ledger().sequence() + 10_000));

    let expiry = (env.ledger().sequence() as u64) + 1_000;
    client.delegate_contribution_rights(&member, &0u32, &proxy, &expiry);

    // Proxy contributes on behalf of member
    client.contribute_via_proxy(&proxy, &member, &token, &1_000i128);
}

#[test]
#[should_panic]
fn test_proxy_cannot_call_with_expired_delegation() {
    let env = Env::default();
    let member = Address::generate(&env);
    let proxy = Address::generate(&env);
    let (client, _admin, token, _) = setup_rosca(&env, &[member.clone()]);

    // Set expiry in the past
    let expiry = env.ledger().sequence() as u64;
    client.delegate_contribution_rights(&member, &0u32, &proxy, &(expiry + 1));

    // Advance ledger past expiry
    env.ledger().set_sequence_number(expiry as u32 + 10);

    client.contribute_via_proxy(&proxy, &member, &token, &1_000i128);
}

#[test]
#[should_panic]
fn test_wrong_proxy_cannot_contribute() {
    let env = Env::default();
    let member = Address::generate(&env);
    let proxy = Address::generate(&env);
    let impostor = Address::generate(&env);
    let (client, _admin, token, _) = setup_rosca(&env, &[member.clone()]);

    let expiry = (env.ledger().sequence() as u64) + 1_000;
    client.delegate_contribution_rights(&member, &0u32, &proxy, &expiry);

    // Impostor tries to contribute — should fail
    client.contribute_via_proxy(&impostor, &member, &token, &1_000i128);
}
