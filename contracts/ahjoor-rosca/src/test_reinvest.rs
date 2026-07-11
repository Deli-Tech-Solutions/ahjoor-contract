#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, token
};

fn setup_with_members<'a>(n: usize) -> (Env, AhjoorContractClient<'a>, Address, Address, soroban_sdk::Vec<Address>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..n {
        members.push_back(Address::generate(&env));
    }

    client.init(
        &admin,
        &members,
        &100, // contribution amount
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

    (env, client, admin, token_admin, members)
}

#[test]
fn test_auto_reinvest_flow() {
    let (env, client, _admin, token_addr, members) = setup_with_members(2);
    let token_client = token::Client::new(&env, &token_addr);

    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();

    // Give members some tokens
    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&member1, &1000);
    token_admin_client.mint(&member2, &1000);

    // Member 1 sets reinvest preference
    client.set_reinvest_preference(&member1, &true);
    assert!(client.get_reinvest_preference(&member1));

    // Round 0: Member 1 is recipient
    client.contribute(&member1, &token_addr, &100);
    client.contribute(&member2, &token_addr, &100);

    // Member 1 was recipient. Pot = 200.
    // Instead of receiving 200, Member 1 reinvests.
    // Member 1 balance should be 900 (1000 - 100 contribution).
    assert_eq!(token_client.balance(&member1), 900);

    // Next round (Round 1) should have Member 1 as already paid
    let (round, paid, _, _, _) = client.get_state();
    assert_eq!(round, 1);
    assert!(paid.contains(&member1));

    // Member 1's contribution for Round 1 should be 200 (reinvested amount)
    let (paid_amt, remaining) = client.get_member_contribution_status(&member1);
    assert_eq!(paid_amt, 200);
    assert_eq!(remaining, -100); // reinvested amount exceeds required contribution
}

#[test]
fn test_toggle_reinvest_preference() {
    let (env, client, _admin, _token_addr, members) = setup_with_members(2);
    let member1 = members.get(0).unwrap();

    client.set_reinvest_preference(&member1, &true);
    assert!(client.get_reinvest_preference(&member1));

    client.set_reinvest_preference(&member1, &false);
    assert!(!client.get_reinvest_preference(&member1));
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #33)")] // ContributionWindowClosed
fn test_cannot_set_reinvest_preference_after_deadline() {
    let (env, client, _admin, _token_addr, members) = setup_with_members(2);
    let member1 = members.get(0).unwrap();

    env.ledger().set_timestamp(4000); // Past deadline (3600)
    client.set_reinvest_preference(&member1, &true);
}


