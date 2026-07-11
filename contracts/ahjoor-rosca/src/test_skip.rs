#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

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
fn test_skip_success_pays_fee_and_excludes_defaulter() {
    let (env, client, admin, token_admin, token_client, _token_admin_client, members) = 
        setup_with_members(3, 1000);

    let skip_fee = 50;
    let contrib_amt = 100;

    client.init(
        &admin,
        &members,
        &contrib_amt,
        &token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 10,
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
            skip_fee,
            max_skips_per_cycle: 1,
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

    let member_skipping = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();
    let member3 = members.get(2).unwrap();

    env.ledger().set_timestamp(100);

    // Member 1 skips
    client.request_skip(&member_skipping, &0);
    
    // Check fee deducted
    assert_eq!(token_client.balance(&member_skipping), 950); // 1000 - 50

    // Others contribute
    client.contribute(&member2, &token_admin, &contrib_amt);
    client.contribute(&member3, &token_admin, &contrib_amt);

    // Round ends after deadline passed (since member 1 skipped, they won't contribute, so we need to finalize)
    env.ledger().set_timestamp(4000);
    client.finalize_round();

    // Check member 1 is NOT a defaulter
    let status = client.get_member_status(&member_skipping);
    assert_eq!(status.default_count, 0);

    // Check payout: Member 1 was supposed to be recipient (round 0), but skipped.
    // So member 2 should get it.
    // Pot = 100 (m2) + 100 (m3) + 50 (skip fee) = 250
    assert_eq!(token_client.balance(&member2), 1150); // 900 (after contrib) + 250 (pot)
    
    // Round advanced
    let (current_round, _, _, _, _) = client.get_state();
    assert_eq!(current_round, 1);
}

#[test]
fn test_skip_limit_enforced() {
    let (env, client, admin, token_admin, _token_client, _token_admin_client, members) = 
        setup_with_members(2, 1000);

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
            max_members: None,
            skip_fee: 10,
            max_skips_per_cycle: 1,
            voting_mode: VotingMode::Equal,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
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
    env.ledger().set_timestamp(100);

    // First skip - success
    client.request_skip(&member, &0);

    // Second skip in same cycle (cycle 0) - fail
    let result = client.try_request_skip(&member, &1);
    assert_eq!(result.unwrap_err().unwrap(), ExtError::SkipLimitReached.into());
}

#[test]
fn test_skip_deadline_enforced() {
    let (env, client, admin, token_admin, _token_client, _token_admin_client, members) = 
        setup_with_members(2, 1000);

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
            skip_fee: 10,
            max_skips_per_cycle: 5,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
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
    
    // Past deadline
    env.ledger().set_timestamp(4000);
    
    let result = client.try_request_skip(&member, &0);
    assert_eq!(result.unwrap_err().unwrap(), Error::ContributionWindowClosed.into());
}

#[test]
fn test_cannot_skip_after_contribution() {
    let (env, client, admin, token_admin, _token_client, _token_admin_client, members) = 
        setup_with_members(2, 1000);

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
            skip_fee: 10,
            max_skips_per_cycle: 5,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
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
    env.ledger().set_timestamp(100);
    
    client.contribute(&member, &token_admin, &100);
    
    let result = client.try_request_skip(&member, &0);
    assert_eq!(result.unwrap_err().unwrap(), Error::AlreadyContributed.into());
}





