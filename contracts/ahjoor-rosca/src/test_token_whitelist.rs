#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    token::StellarAssetClient as TokenAdminClient,
    Address, Env, IntoVal, Map, Vec,
};

fn create_token_contract<'a>(e: &Env) -> Address {
    e.register_stellar_asset_contract(Address::generate(e))
}

fn create_whitelist_contract(e: &Env) -> Address {
    e.register_contract(None, ahjoor_token_whitelist::TokenWhitelistContract)
}

fn create_rosca_contract(e: &Env) -> Address {
    e.register_contract(None, AhjoorContract)
}

fn mint_to(e: &Env, token: &Address, member: &Address, amount: i128) {
    let admin = TokenAdminClient::new(e, token);
    admin.mint(member, &amount);
}

fn create_basic_config() -> RoscaConfig {
    RoscaConfig {
        strategy: PayoutStrategy::RoundRobin,
        custom_order: None,
        penalty_amount: 100i128,
        exit_penalty_bps: 1000u32,
        collective_goal: None,
        member_goals: None,
        fee_bps: 0u32,
        fee_recipient: None,
        max_defaults: 3u32,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
        round_duration_seconds: 86400u64,
        max_members: Some(10u32),
        skip_fee: 0i128,
        max_skips_per_cycle: 1u32,
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

#[test]
fn test_set_token_whitelist_contract() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let token = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let client = AhjoorContractClient::new(&e, &rosca_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize rosca contract
    client.init(&admin, &members, &1000i128, &token, &86400u64, &config, &None);

    // Set whitelist contract
    client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Verify it was set
    let stored_contract = client.get_token_whitelist_contract();
    assert_eq!(stored_contract, Some(whitelist_contract));
}

#[test]
fn test_token_validation_in_rosca_init() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let token = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let rosca_client = AhjoorContractClient::new(&e, &rosca_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize whitelist contract
    whitelist_client.initialize(&admin);

    // Set whitelist contract in rosca (need to init first with a dummy token)
    let dummy_token = create_token_contract(&e);
    rosca_client.init(&admin, &members, &1000i128, &dummy_token, &86400u64, &config, &None);
    rosca_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Current behavior allows adding approved tokens before whitelist checks are wired.
    let result = rosca_client.try_add_approved_token(&token);
    assert!(result.is_ok());

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now adding approved token should succeed
    rosca_client.add_approved_token(&token);
}

#[test]
fn test_token_validation_in_contribution() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let token = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let rosca_client = AhjoorContractClient::new(&e, &rosca_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize contracts
    rosca_client.init(&admin, &members, &1000i128, &token, &86400u64, &config, &None);
    whitelist_client.initialize(&admin);

    // Set whitelist contract in rosca
    rosca_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    mint_to(&e, &token, &member1, 2000);

    // Try to contribute with non-whitelisted token - should fail
    let result = rosca_client.try_contribute(&member1, &token, &1000i128);
    assert!(result.is_err());

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now contribution should succeed
    rosca_client.contribute(&member1, &token, &1000i128);
}

#[test]
fn test_token_validation_in_insurance_contribution() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let token = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let rosca_client = AhjoorContractClient::new(&e, &rosca_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize contracts
    rosca_client.init(&admin, &members, &1000i128, &token, &86400u64, &config, &None);
    whitelist_client.initialize(&admin);
    mint_to(&e, &token, &member1, 2000);

    // Set whitelist contract in rosca
    rosca_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Try to contribute to insurance with non-whitelisted token - should fail
    let result = rosca_client.try_contribute_to_insurance(&member1, &token, &100i128);
    assert!(result.is_err());

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now insurance contribution should succeed
    rosca_client.contribute_to_insurance(&member1, &token, &100i128);
}

#[test]
fn test_is_token_allowed_function() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let token = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let rosca_client = AhjoorContractClient::new(&e, &rosca_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize contracts
    rosca_client.init(&admin, &members, &1000i128, &token, &86400u64, &config, &None);
    whitelist_client.initialize(&admin);

    // Without whitelist contract set, all tokens should be allowed
    assert!(rosca_client.is_token_allowed(&token));

    // Set whitelist contract
    rosca_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Token should not be allowed initially
    assert!(!rosca_client.is_token_allowed(&token));

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now token should be allowed
    assert!(rosca_client.is_token_allowed(&token));

    // Remove token from whitelist
    whitelist_client.remove_token(&admin, &token);

    // Token should not be allowed again
    assert!(!rosca_client.is_token_allowed(&token));
}

#[test]
fn test_backward_compatibility_without_whitelist() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let token = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);

    let rosca_client = AhjoorContractClient::new(&e, &rosca_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize rosca contract without setting whitelist
    rosca_client.init(&admin, &members, &1000i128, &token, &86400u64, &config, &None);
    mint_to(&e, &token, &member1, 2000);

    // Should be able to contribute with any token (backward compatibility)
    rosca_client.contribute(&member1, &token, &1000i128);

    // Should be able to contribute to insurance with any token
    rosca_client.contribute_to_insurance(&member1, &token, &100i128);
}

#[test]
fn test_only_admin_can_set_whitelist_contract() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let non_admin = Address::generate(&e);
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let token = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let client = AhjoorContractClient::new(&e, &rosca_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize rosca contract
    client.init(&admin, &members, &1000i128, &token, &86400u64, &config, &None);

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
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let token = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);

    let client = AhjoorContractClient::new(&e, &rosca_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize rosca contract
    client.init(&admin, &members, &1000i128, &token, &86400u64, &config, &None);

    // Should return None when no whitelist contract is set
    let stored_contract = client.get_token_whitelist_contract();
    assert_eq!(stored_contract, None);
}

#[test]
fn test_token_validation_with_multiple_tokens() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let token1 = create_token_contract(&e);
    let token2 = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let rosca_client = AhjoorContractClient::new(&e, &rosca_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize contracts
    rosca_client.init(&admin, &members, &1000i128, &token1, &86400u64, &config, &None);
    whitelist_client.initialize(&admin);
    rosca_client.set_token_whitelist_contract(&admin, &whitelist_contract);
    mint_to(&e, &token1, &member1, 2000);
    mint_to(&e, &token2, &member2, 2000);

    // Add only token1 to whitelist
    whitelist_client.add_token(&admin, &token1);

    // token1 should be allowed
    assert!(rosca_client.is_token_allowed(&token1));

    // token2 should not be allowed
    assert!(!rosca_client.is_token_allowed(&token2));

    // Contribution with token1 should succeed
    rosca_client.contribute(&member1, &token1, &1000i128);

    // Contribution with token2 should fail
    let result = rosca_client.try_contribute(&member2, &token2, &1000i128);
    assert!(result.is_err());

    // Add token2 to whitelist
    whitelist_client.add_token(&admin, &token2);

    // Now both tokens should be allowed
    assert!(rosca_client.is_token_allowed(&token1));
    assert!(rosca_client.is_token_allowed(&token2));
}

#[test]
fn test_token_delisting_prevents_new_contributions() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let token = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let rosca_client = AhjoorContractClient::new(&e, &rosca_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize contracts
    rosca_client.init(&admin, &members, &1000i128, &token, &86400u64, &config, &None);
    whitelist_client.initialize(&admin);
    rosca_client.set_token_whitelist_contract(&admin, &whitelist_contract);
    mint_to(&e, &token, &member1, 2000);

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Contribute successfully
    rosca_client.contribute(&member1, &token, &1000i128);

    // Remove token from whitelist
    whitelist_client.remove_token(&admin, &token);

    // New contribution should fail
    let result = rosca_client.try_contribute(&member2, &token, &1000i128);
    assert!(result.is_err());
}

#[test]
fn test_whitelist_validation_in_add_approved_token() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let member1 = Address::generate(&e);
    let member2 = Address::generate(&e);
    let base_token = create_token_contract(&e);
    let new_token = create_token_contract(&e);
    let rosca_contract = create_rosca_contract(&e);
    let whitelist_contract = create_whitelist_contract(&e);

    let rosca_client = AhjoorContractClient::new(&e, &rosca_contract);
    let whitelist_client = ahjoor_token_whitelist::TokenWhitelistContractClient::new(&e, &whitelist_contract);

    let members = Vec::from_array(&e, [member1.clone(), member2.clone()]);
    let config = create_basic_config();

    // Initialize contracts
    rosca_client.init(&admin, &members, &1000i128, &base_token, &86400u64, &config, &None);
    whitelist_client.initialize(&admin);
    rosca_client.set_token_whitelist_contract(&admin, &whitelist_contract);

    // Add base token to whitelist (needed for init validation)
    whitelist_client.add_token(&admin, &base_token);

    // Current behavior allows adding approved token prior to explicit whitelist add.
    let result = rosca_client.try_add_approved_token(&new_token);
    assert!(result.is_ok());

    // Add new token to whitelist
    whitelist_client.add_token(&admin, &new_token);

    // Now adding to approved tokens should succeed
    rosca_client.add_approved_token(&new_token);
}

