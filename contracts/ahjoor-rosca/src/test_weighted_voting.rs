#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient as TokenAdminClient},
    Address, Env,
};

fn setup_with_members<'a>(
    n: usize,
    voting_mode: VotingMode,
) -> (
    Env,
    AhjoorContractClient<'a>,
    Address,
    Address,
    TokenClient<'a>,
    TokenAdminClient<'a>,
    soroban_sdk::Vec<Address>,
) {
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
        members.push_back(Address::generate(&env));
    }

    client.init(
        &admin,
        &members,
        &1000, // contribution amount
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
            voting_mode,
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

    (env, client, admin, token_admin, token_client, token_admin_client, members)
}

#[test]
fn test_weighted_voting_power() {
    let (env, client, admin, token_addr, _token_client, token_admin_client, members) =
        setup_with_members(3, VotingMode::WeightedByContributions);

    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();
    let member3 = members.get(2).unwrap();

    // Give members some tokens and set a higher tier for member1 so they can contribute more.
    token_admin_client.mint(&member1, &3000);
    token_admin_client.mint(&member2, &2000);
    client.set_member_tier(&admin, &member1, &20000); // 2x required contribution

    // Members contribute to gain weight.
    client.contribute(&member1, &token_addr, &200);
    client.contribute(&member2, &token_addr, &100);

    // member3: 0 contributions

    // Verify weights
    assert_eq!(client.get_member_voting_weight(&member1), 200);
    assert_eq!(client.get_member_voting_weight(&member2), 100);
    assert_eq!(client.get_member_voting_weight(&member3), 0);

    // Test voting
    client.create_proposal(
        &member1,
        &ProposalType::RuleChange,
        &soroban_sdk::String::from_str(&env, "test"),
        &admin,
        &86400,
        &None,
    );
    let prop_id = 0;

    // Member 3 cannot vote (0 weight)
    // client.vote_on_proposal(&member3, &prop_id, &true); // This should panic

    // Member 2 votes (weight 1000)
    client.vote_on_proposal(&member2, &prop_id, &false);
    
    let prop = client.get_proposal(&prop_id).unwrap();
    assert_eq!(prop.votes_against, 100);

    // Member 1 votes (weight 2000)
    client.vote_on_proposal(&member1, &prop_id, &true);
    let prop = client.get_proposal(&prop_id).unwrap();
    assert_eq!(prop.votes_for, 200);

    // Quorum check:
    // Total possible votes = 2000 + 1000 + 0 = 3000
    // Required quorum = 51% of 3000 = 1530
    // Total votes cast = 2000 (for) + 1000 (against) = 3000
    // 3000 >= 1530 -> Quorum met
    // 2000 > 1000 -> Approved

    env.ledger().set_timestamp(100000);
    client.execute_proposal(&prop_id);

    let prop = client.get_proposal(&prop_id).unwrap();
    assert_eq!(prop.status, ProposalStatus::Executed);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #56)")] // InsufficientWeight
fn test_zero_contribution_cannot_vote_in_weighted_mode() {
    let (env, client, _admin, _token_addr, _token_client, _token_admin_client, members) =
        setup_with_members(3, VotingMode::WeightedByContributions);
    let member1 = members.get(0).unwrap();
    let member3 = members.get(2).unwrap();

    client.create_proposal(
        &member1,
        &ProposalType::RuleChange,
        &soroban_sdk::String::from_str(&env, "test"),
        &member1,
        &86400,
        &None,
    );
    
    client.vote_on_proposal(&member3, &0, &true);
}

#[test]
fn test_equal_voting_mode_preserves_behavior() {
    let (env, client, _admin, token_addr, _token_client, token_admin_client, members) =
        setup_with_members(3, VotingMode::Equal);
    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();

    token_admin_client.mint(&member1, &2000);
    token_admin_client.mint(&member2, &1000);
    client.contribute(&member1, &token_addr, &1000);

    // member1 weight should be 1 despite contribution
    assert_eq!(client.get_member_voting_weight(&member1), 1);

    client.create_proposal(
        &member1,
        &ProposalType::RuleChange,
        &soroban_sdk::String::from_str(&env, "test"),
        &member1,
        &86400,
        &None,
    );

    client.vote_on_proposal(&member2, &0, &true);
    let prop = client.get_proposal(&0).unwrap();
    assert_eq!(prop.votes_for, 1);
}

/// #398: A member with an active contribution-weight delegation must not be
/// able to cast a direct vote — the contract should return
/// CannotVoteWithActiveDelegation.
#[test]
fn test_delegated_member_cannot_double_vote() {
    use soroban_sdk::testutils::Ledger;

    let (env, client, _admin, token_addr, _token_client, token_admin_client, members) =
        setup_with_members(3, VotingMode::WeightedByContributions);

    let delegator = members.get(0).unwrap();
    let delegate  = members.get(1).unwrap();
    let proposer  = members.get(2).unwrap();

    token_admin_client.mint(&delegator, &2000);
    token_admin_client.mint(&delegate,  &2000);
    token_admin_client.mint(&proposer,  &2000);

    // Build up contribution weight
    client.contribute(&delegator, &token_addr, &1000);
    client.contribute(&delegate,  &token_addr, &1000);

    // Advance ledger; delegate vote expires at sequence 1_000_000
    env.ledger().with_mut(|l| {
        l.sequence_number = 100;
        l.timestamp       = 200;
    });

    // Delegator sets up a contribution delegation to delegate
    client.delegate_contribution_vote(&delegator, &delegate, &1_000_000u64);

    // Create a proposal
    client.create_proposal(
        &proposer,
        &ProposalType::RuleChange,
        &soroban_sdk::String::from_str(&env, "test delegation"),
        &proposer,
        &86400,
        &None,
    );

    // Delegator tries to vote directly — must be rejected
    let result = client.try_vote_on_proposal(&delegator, &0, &true);
    assert!(
        result.is_err(),
        "delegator with active contrib delegation should not be able to vote directly"
    );

    // Delegate can vote; delegator's weight is automatically included
    client.vote_on_proposal(&delegate, &0, &true);

    let prop = client.get_proposal(&0).unwrap();
    // Both weights (1000 each) should be counted
    assert_eq!(prop.votes_for, 2000, "delegate vote should carry delegator weight too");

    // Delegator can no longer vote (already marked as voted via delegation)
    let result2 = client.try_vote_on_proposal(&delegator, &0, &true);
    assert!(result2.is_err(), "delegator should not be able to vote after delegate already voted");
}



