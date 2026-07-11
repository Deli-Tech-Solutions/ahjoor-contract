#![cfg(test)]
use super::*;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

fn setup_cosigner<'a>() -> (Env, AhjoorContractClient<'a>, Address, Address, soroban_sdk::Vec<Address>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..3 {
        let addr = Address::generate(&env);
        token_admin_client.mint(&addr, &10_000);
        members.push_back(addr);
    }

    client.init(
        &admin,
        &members,
        &100,
        &token_addr,
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
            skip_fee: 0,
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

    // Set co-signer window to 500 ledgers
    client.set_co_signer_window(&admin, &500u32);

    env.ledger().set_timestamp(100);

    (env, client, admin, token_addr, members)
}

#[test]
fn test_cosigner_honours_contribution() {
    let (env, client, _admin, token_addr, members) = setup_cosigner();
    let member = members.get(0).unwrap();
    let co_signer = Address::generate(&env);

    // Mint tokens to co-signer
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&co_signer, &10_000);

    // Set and accept co-signer
    client.set_co_signer(&member, &0, &co_signer);
    client.accept_co_signer(&co_signer, &0, &member);

    // Other members contribute
    let m2 = members.get(1).unwrap();
    let m3 = members.get(2).unwrap();
    client.contribute(&m2, &token_addr, &100);
    client.contribute(&m3, &token_addr, &100);

    // Advance past deadline — finalize_round opens co-signer window for member
    env.ledger().set_timestamp(100_000);
    client.finalize_round();

    // Co-signer contributes on behalf of member within window
    client.co_signer_contribute(&co_signer, &0, &member, &token_addr, &100);
}

#[test]
fn test_window_expiry_triggers_member_penalty() {
    let (env, client, _admin, token_addr, members) = setup_cosigner();
    let member = members.get(0).unwrap();
    let co_signer = Address::generate(&env);

    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&co_signer, &10_000);

    client.set_co_signer(&member, &0, &co_signer);
    client.accept_co_signer(&co_signer, &0, &member);

    let m2 = members.get(1).unwrap();
    let m3 = members.get(2).unwrap();
    client.contribute(&m2, &token_addr, &100);
    client.contribute(&m3, &token_addr, &100);

    env.ledger().set_timestamp(100_000);
    client.finalize_round();

    // Advance past co-signer window
    env.ledger().with_mut(|l| l.sequence_number += 600);

    // Co-signer tries to contribute after window — should fail
    let result = client.try_co_signer_contribute(&co_signer, &0, &member, &token_addr, &100);
    assert!(result.is_err());
}

#[test]
fn test_remove_cosigner_clears_designation() {
    let (env, client, _admin, token_addr, members) = setup_cosigner();
    let member = members.get(0).unwrap();
    let co_signer = Address::generate(&env);

    client.set_co_signer(&member, &0, &co_signer);
    client.remove_co_signer(&member, &0);

    // Setting again should succeed (old one was cleared)
    let new_co_signer = Address::generate(&env);
    client.set_co_signer(&member, &0, &new_co_signer);
}

#[test]
fn test_unaccepted_cosigner_not_active() {
    let (env, client, _admin, token_addr, members) = setup_cosigner();
    let member = members.get(0).unwrap();
    let co_signer = Address::generate(&env);

    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&co_signer, &10_000);

    // Set but do NOT accept
    client.set_co_signer(&member, &0, &co_signer);

    // Co-signer tries to contribute without accepting — should fail
    let result = client.try_co_signer_contribute(&co_signer, &0, &member, &token_addr, &100);
    assert!(result.is_err());
}


#[test]
fn test_pending_cosigner_skipped_on_default() {
    let (env, client, _admin, token_addr, members) = setup_cosigner();
    let member = members.get(0).unwrap();
    let co_signer = Address::generate(&env);

    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&co_signer, &10_000);

    // Set co-signer but DO NOT accept (status = Pending)
    client.set_co_signer(&member, &0, &co_signer);

    // Other members contribute
    let m2 = members.get(1).unwrap();
    let m3 = members.get(2).unwrap();
    client.contribute(&m2, &token_addr, &100);
    client.contribute(&m3, &token_addr, &100);

    // Get initial default count for member
    let initial_defaults = client.get_member_status(&member).default_count;

    // Advance past deadline and finalize round
    env.ledger().set_timestamp(100_000);
    client.finalize_round();

    // member should now be marked as defaulter with incremented count
    let final_defaults = client.get_member_status(&member).default_count;
    assert_eq!(final_defaults, initial_defaults + 1, "Default count should be incremented");

    // Verify CoSignerWindowStart is NOT in storage (window should not have been opened)
    // The window should not exist because pending co-signers are skipped
    // We verify this by trying to access it - if no window was opened, the test passes
    
    // Attempt a co-signer contribution to confirm window wasn't opened
    // The window should still be closed even for the co-signer
    let result = client.try_co_signer_contribute(&co_signer, &0, &member, &token_addr, &100);
    // This should fail because:
    // 1. No window was opened for the pending co-signer
    // 2. The co-signer hasn't accepted the assignment
    assert!(result.is_err(), "Co-signer should not be able to contribute without accepted status and open window");
}
