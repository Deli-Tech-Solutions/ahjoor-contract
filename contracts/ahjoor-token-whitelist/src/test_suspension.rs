#![cfg(test)]
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env};

use crate::{TokenWhitelistContract, TokenWhitelistContractClient};

fn setup(env: &Env) -> (TokenWhitelistContractClient<'static>, Address) {
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

fn reason(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[9u8; 32])
}

#[test]
fn test_suspension_active_blocks_is_token_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    assert!(client.is_token_allowed(&token));

    // Suspend for 1000 ledgers
    client.suspend_token_timed(&admin, &token, &1000u32, &reason(&env));
    assert!(!client.is_token_allowed(&token));
}

#[test]
fn test_auto_reinstatement_on_expiry_query() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    client.suspend_token_timed(&admin, &token, &5u32, &reason(&env));
    assert!(!client.is_token_allowed(&token));

    // Advance ledger past expiry
    env.ledger().with_mut(|l| l.sequence_number += 10);

    // Lazy reinstatement: is_token_allowed clears suspension and returns true
    assert!(client.is_token_allowed(&token));

    // Suspension record cleared
    assert!(client.get_token_suspension(&token).is_none());
}

#[test]
fn test_lift_suspension_early() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    client.suspend_token_timed(&admin, &token, &1000u32, &reason(&env));
    assert!(!client.is_token_allowed(&token));

    client.lift_token_suspension(&admin, &token);
    assert!(client.is_token_allowed(&token));
    assert!(client.get_token_suspension(&token).is_none());
}

#[test]
fn test_extend_suspension() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    let initial_seq = env.ledger().sequence();
    client.suspend_token_timed(&admin, &token, &100u32, &reason(&env));

    let susp = client.get_token_suspension(&token).unwrap();
    assert_eq!(susp.expiry_ledger, initial_seq + 100);

    client.extend_token_suspension(&admin, &token, &50u32);
    let susp = client.get_token_suspension(&token).unwrap();
    assert_eq!(susp.expiry_ledger, initial_seq + 150);
}

#[test]
fn test_suspension_history_stored() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    client.suspend_token_timed(&admin, &token, &100u32, &reason(&env));

    let history = client.get_suspension_history(&token);
    assert_eq!(history.len(), 1);
    // History entry contains start_ledger, expiry_ledger, and reason_hash
    let entry = history.get(0).unwrap();
    assert_eq!(entry.expiry_ledger, env.ledger().sequence() + 100);
}

#[test]
fn test_suspension_history_capped_at_10() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);

    client.add_token(&admin, &token);

    for i in 0u32..12 {
        // Lift previous suspension before adding new one
        if i > 0 {
            client.lift_token_suspension(&admin, &token);
        }
        client.suspend_token_timed(&admin, &token, &(100 + i), &reason(&env));
    }

    let history = client.get_suspension_history(&token);
    assert_eq!(history.len(), 10);
}

#[test]
fn test_non_suspended_token_baseline_unchanged() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    // No suspension — should still be allowed
    assert!(client.is_token_allowed(&token));

    client.remove_token(&admin, &token);
    assert!(!client.is_token_allowed(&token));
}

#[test]
#[should_panic(expected = "No active suspension")]
fn test_lift_nonexistent_suspension_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.lift_token_suspension(&admin, &token);
}

#[test]
#[should_panic(expected = "No active suspension")]
fn test_extend_nonexistent_suspension_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.extend_token_suspension(&admin, &token, &50u32);
}

/// Regression test for the "per-contract allowlist bypasses global
/// suspension" bug: a no-expiry `ContractTokenAllowlist` entry must never let
/// a consumer contract keep using a token that has an active
/// `suspend_token_timed` suspension.
#[test]
fn test_suspend_token_timed_always_wins_over_contract_allowlist() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let contract_id = Address::generate(&env);

    client.add_token(&admin, &token);
    client.set_contract_token(&admin, &contract_id, &token, &None);
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));

    client.suspend_token_timed(&admin, &token, &1000u32, &reason(&env));

    // Global check: suspended.
    assert!(!client.is_token_allowed(&token));
    // Per-contract check: suspension wins despite the no-expiry allowlist entry.
    assert!(!client.is_token_allowed_for_contract(&contract_id, &token));
}
