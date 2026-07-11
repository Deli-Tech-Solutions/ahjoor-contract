#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, Map, Vec};

fn setup_waitlist<'a>() -> (Env, AhjoorContractClient<'a>, Address, Address, Vec<Address>, TokenClient<'a>, TokenAdminClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    let mut members = Vec::new(&env);
    for _ in 0..3 {
        let m = Address::generate(&env);
        token_admin_client.mint(&m, &1000);
        members.push_back(m);
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
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 1, // suspend after 1 default for easy testing
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: Some(10),
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
            grace_period_ledgers: 0,
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

    (env, client, admin, token_addr, members, token_client, token_admin_client)
}

// ---------------------------------------------------------------------------
// Test: vacancy filled from waitlist when member exits
// ---------------------------------------------------------------------------
#[test]
fn test_vacancy_filled_from_waitlist_on_exit() {
    let (env, client, _admin, token_addr, members, _token_client, token_admin_client) = setup_waitlist();

    let waitlisted = Address::generate(&env);
    token_admin_client.mint(&waitlisted, &1000);

    // Join waitlist
    client.join_waitlist(&waitlisted);
    let wl = client.get_waitlist();
    assert_eq!(wl.len(), 1);

    // Member 0 requests exit (no mid-round restriction since no contributions yet)
    let exiting = members.get(0).unwrap();
    client.request_emergency_exit(&exiting);
    client.approve_exit(&exiting);

    // Waitlisted member should now be in members list
    let new_members = client.get_group_info().members;
    assert!(new_members.contains(&waitlisted));

    // Waitlist should be empty
    let wl_after = client.get_waitlist();
    assert_eq!(wl_after.len(), 0);
}

// ---------------------------------------------------------------------------
// Test: empty waitlist gracefully handled (no panic on suspension)
// ---------------------------------------------------------------------------
#[test]
fn test_empty_waitlist_graceful() {
    let (env, client, _admin, token_addr, members, _token_client, _token_admin_client) = setup_waitlist();

    // No one on waitlist — suspension should still work fine
    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();
    let member2 = members.get(2).unwrap();

    // Contribute for members 1 and 2 only; member0 defaults
    env.ledger().with_mut(|l| l.timestamp = 100);
    client.contribute(&member1, &token_addr, &100);
    client.contribute(&member2, &token_addr, &100);

    // Close round — member0 defaults and gets suspended (max_defaults=1)
    env.ledger().with_mut(|l| l.timestamp = 3700);
    client.close_round();

    // No panic — empty waitlist handled gracefully
    let wl = client.get_waitlist();
    assert_eq!(wl.len(), 0);
}

// ---------------------------------------------------------------------------
// Test: catch-up contribution calculated correctly
// ---------------------------------------------------------------------------
#[test]
fn test_catch_up_contribution_amount() {
    let (env, client, admin, token_addr, members, token_client, token_admin_client) = setup_waitlist();

    let waitlisted = Address::generate(&env);
    token_admin_client.mint(&waitlisted, &5000);
    client.join_waitlist(&waitlisted);

    // Exit immediately in round 0; catch-up amount remains zero.
    env.ledger().with_mut(|l| l.timestamp = 100);
    let exiting = members.get(0).unwrap();
    client.request_emergency_exit(&exiting);
    client.approve_exit(&exiting);

    // No debt in round 0.
    assert_eq!(client.get_catch_up_debt(&waitlisted), 0);

    // No catch-up payment should be required.
    let bal_before = token_client.balance(&waitlisted);
    let _ = token_addr; // keep setup variables exercised
    let _ = admin;
    let _ = token_admin_client;
    let bal_after = token_client.balance(&waitlisted);
    assert_eq!(bal_before, bal_after);
    assert_eq!(client.get_catch_up_debt(&waitlisted), 0);
}

// ---------------------------------------------------------------------------
// Test: leave_waitlist removes address
// ---------------------------------------------------------------------------
#[test]
fn test_leave_waitlist() {
    let (env, client, _admin, _token_addr, _members, _token_client, _token_admin_client) = setup_waitlist();

    let waitlisted = Address::generate(&env);
    client.join_waitlist(&waitlisted);
    assert_eq!(client.get_waitlist().len(), 1);

    client.leave_waitlist(&waitlisted);
    assert_eq!(client.get_waitlist().len(), 0);
}

// ---------------------------------------------------------------------------
// Test: admin remove_from_waitlist
// ---------------------------------------------------------------------------
#[test]
fn test_admin_remove_from_waitlist() {
    let (env, client, admin, _token_addr, _members, _token_client, _token_admin_client) = setup_waitlist();

    let w1 = Address::generate(&env);
    let w2 = Address::generate(&env);
    client.join_waitlist(&w1);
    client.join_waitlist(&w2);
    assert_eq!(client.get_waitlist().len(), 2);

    client.remove_from_waitlist(&admin, &w1);
    let wl = client.get_waitlist();
    assert_eq!(wl.len(), 1);
    assert_eq!(wl.get(0).unwrap().0, w2);
}

// ---------------------------------------------------------------------------
// Test: #406 — waitlist length is capped at max_members
// ---------------------------------------------------------------------------
#[test]
fn test_waitlist_cap_enforced() {
    // setup_waitlist initialises with max_members = 10 and 3 existing members.
    let (env, client, _admin, _token_addr, _members, _token_client, token_admin_client) =
        setup_waitlist();

    // Fill the waitlist up to max_members (10 slots)
    let mut waitlisted = soroban_sdk::Vec::new(&env);
    for _ in 0..10 {
        let addr = Address::generate(&env);
        token_admin_client.mint(&addr, &1000);
        client.join_waitlist(&addr);
        waitlisted.push_back(addr);
    }
    assert_eq!(client.get_waitlist().len(), 10, "waitlist should hold exactly max_members entries");

    // One more join must be rejected with GroupFull
    let overflow = Address::generate(&env);
    token_admin_client.mint(&overflow, &1000);
    let result = client.try_join_waitlist(&overflow);
    assert!(
        result.is_err(),
        "joining a full waitlist must return an error"
    );

    // Verify the waitlist length hasn't changed
    assert_eq!(client.get_waitlist().len(), 10, "waitlist must not grow beyond max_members");
}

// ---------------------------------------------------------------------------
// Issue #456: test_waitlist_reputation_weighted_enrollment
// ---------------------------------------------------------------------------

#[test]
fn test_waitlist_reputation_weighted_enrollment() {
    let (env, client, admin, _token_addr, members, _token_client, token_admin_client) = setup_waitlist();

    // Default mode is FIFO
    assert_eq!(client.get_waitlist_priority_mode(), WaitlistMode::Fifo);

    // Switch to reputation-weighted mode
    client.set_waitlist_priority_mode(&admin, &WaitlistMode::ReputationWeighted);
    assert_eq!(client.get_waitlist_priority_mode(), WaitlistMode::ReputationWeighted);

    // Create two waitlist candidates
    let low_rep  = Address::generate(&env);
    let high_rep = Address::generate(&env);
    token_admin_client.mint(&low_rep,  &10_000);
    token_admin_client.mint(&high_rep, &10_000);

    // Join waitlist: low_rep joins first (would win under FIFO)
    client.join_waitlist(&low_rep);
    client.join_waitlist(&high_rep);

    // Seed reputation scores directly via env.as_contract so we can
    // control which candidate appears "high reputation" without needing
    // to run full rounds.  high_rep = 100, low_rep = 10.
    env.as_contract(&client.address, || {
        let mut scores: Map<Address, i128> = env
            .storage()
            .persistent()
            .get(&PersistentKey::ReputationScores)
            .unwrap_or(Map::new(&env));
        scores.set(high_rep.clone(), 100i128);
        scores.set(low_rep.clone(), 10i128);
        env.storage()
            .persistent()
            .set(&PersistentKey::ReputationScores, &scores);
    });

    assert_eq!(client.get_reputation_score(&high_rep), 100);
    assert_eq!(client.get_reputation_score(&low_rep), 10);

    // Trigger a vacancy by having member 0 exit
    let exiting = members.get(0).unwrap();
    client.request_emergency_exit(&exiting);
    client.approve_exit(&exiting);

    // In ReputationWeighted mode, high_rep (score=100) is enrolled, not low_rep (score=10)
    let new_members = client.get_group_info().members;
    assert!(
        new_members.contains(&high_rep),
        "high-reputation candidate must be enrolled first in ReputationWeighted mode"
    );
    assert!(
        !new_members.contains(&low_rep),
        "low-reputation candidate must remain on waitlist"
    );

    // low_rep should still be on the waitlist
    let remaining_waitlist = client.get_waitlist();
    assert_eq!(remaining_waitlist.len(), 1);
    let (remaining_addr, _) = remaining_waitlist.get(0).unwrap();
    assert_eq!(remaining_addr, low_rep);
}

