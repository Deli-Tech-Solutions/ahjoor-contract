#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, vec,
};

/// Helper to create a test setup with members
fn setup_with_members<'a>(n: usize, mint_amount: i128) -> (Env, AhjoorContractClient<'a>, Address, Address, TokenClient<'a>, TokenAdminClient<'a>, soroban_sdk::Vec<Address>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_admin);
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..n {
        let addr = Address::generate(&env);
        if mint_amount > 0 {
            token_admin_client.mint(&addr, &mint_amount);
        }
        members.push_back(addr);
    }

    (env, client, admin, token_admin, token_client, token_admin_client, members)
}

#[test]
fn test_tiered_contributions() {
    let (env, client, admin, token_admin, token_client, _, members) = 
        setup_with_members(2, 2000);

    let base_amount = 100;
    client.init(
        &admin,
        &members,
        &base_amount,
        &token_admin,
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

    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();

    // Set member2 to 2x tier (20000 bps)
    client.set_member_tier(&admin, &member2, &20000);

    // Member1 (default 1x) contributes base_amount
    client.contribute(&member1, &token_admin, &base_amount);
    assert_eq!(token_client.balance(&member1), 2000 - base_amount);

    // Member2 (2x tier) must contribute 2 * base_amount
    // Try contributing only base_amount first
    client.contribute(&member2, &token_admin, &base_amount);
    assert_eq!(token_client.balance(&member2), 2000 - base_amount);
    
    // Member2 should not be marked as paid yet
    let (_, paid, _, _, _) = client.get_state();
    assert_eq!(paid.len(), 1);
    assert!(paid.contains(&member1));
    assert!(!paid.contains(&member2));

    // Member2 contributes the remaining base_amount
    client.contribute(&member2, &token_admin, &base_amount);
    assert_eq!(token_client.balance(&member2), 2000 - 2 * base_amount);

    // Now round should be complete. Pot = 100 + 200 = 300.
    // Recipient is member1 (index 0).
    assert_eq!(token_client.balance(&member1), (2000 - 100) + 300);
}

#[test]
fn test_invalid_tier_rejected() {
    let (env, client, admin, token_admin, _, _, members) = 
        setup_with_members(1, 1000);

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
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

    let member = members.get(0).unwrap();
    
    // Tier 0 is invalid
    let result = client.try_set_member_tier(&admin, &member, &0);
    assert_eq!(result.unwrap_err().unwrap(), ExtError::InvalidTier.into());
}

#[test]
fn test_mixed_tiers_pot_size() {
    let (env, client, admin, token_admin, token_client, _, members) = 
        setup_with_members(3, 3000);

    let base_amount = 100;
    client.init(
        &admin,
        &members,
        &base_amount,
        &token_admin,
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

    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();
    let member3 = members.get(2).unwrap();

    // Member1: 1x (default) -> 100
    // Member2: 1.5x (15000 bps) -> 150
    // Member3: 3x (30000 bps) -> 300
    client.set_member_tier(&admin, &member2, &15000);
    client.set_member_tier(&admin, &member3, &30000);

    client.contribute(&member1, &token_admin, &100);
    client.contribute(&member2, &token_admin, &150);
    client.contribute(&member3, &token_admin, &300);

    // Total pot = 100 + 150 + 300 = 550
    // Recipient is member1
    assert_eq!(token_client.balance(&member1), (3000 - 100) + 550);
}


