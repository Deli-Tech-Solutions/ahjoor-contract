#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
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

    (env, client, admin, token_admin, members)
}

#[test]
fn test_per_type_quorum_enforced() {
    let (env, client, admin, token_admin, members) = setup_with_members(10);

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

    // Set Global Quorum to 51% (already default, but let's be sure)
    // Actually the init doesn't take quorum, it uses default 51.

    // 1. PenaltyAppeal at 10% (1000 bps)
    client.set_quorum_per_type(&admin, &ProposalType::PenaltyAppeal, &1000);
    
    // 2. MemberRemoval at 67% (6700 bps)
    client.set_quorum_per_type(&admin, &ProposalType::MemberRemoval, &6700);

    let creator = members.get(0).unwrap();

    // Test PenaltyAppeal Quorum
    client.create_proposal(
        &creator,
        &ProposalType::PenaltyAppeal,
        &soroban_sdk::String::from_str(&env, "appeal"),
        &members.get(1).unwrap(),
        &86400,
        &None,
    );
    
    let prop_id_appeal = 0;
    client.vote_on_proposal(&creator, &prop_id_appeal, &true); // 1 vote = 10% of 10 members
    
    env.ledger().set_timestamp(90000); // Past deadline
    client.execute_proposal(&prop_id_appeal);
    
    let prop_appeal = client.get_proposal(&prop_id_appeal).unwrap();
    assert_eq!(prop_appeal.status, ProposalStatus::Executed); // 10% was enough

    // Test MemberRemoval Quorum (needs 67% = 7 votes for 10 members)
    env.ledger().set_timestamp(100);
    client.create_proposal(
        &creator,
        &ProposalType::MemberRemoval,
        &soroban_sdk::String::from_str(&env, "remove"),
        &members.get(2).unwrap(),
        &86400,
        &None,
    );

    let prop_id_removal = 1;
    // Vote 6 times (60%)
    for i in 0..6 {
        client.vote_on_proposal(&members.get(i).unwrap(), &prop_id_removal, &true);
    }

    env.ledger().set_timestamp(100000);
    client.execute_proposal(&prop_id_removal);

    let prop_removal = client.get_proposal(&prop_id_removal).unwrap();
    assert_eq!(prop_removal.status, ProposalStatus::Rejected); // 60% < 67%

    // Create another one and vote 7 times
    env.ledger().set_timestamp(110000);
    client.create_proposal(
        &creator,
        &ProposalType::MemberRemoval,
        &soroban_sdk::String::from_str(&env, "remove 2"),
        &members.get(3).unwrap(),
        &86400,
        &None,
    );
    let prop_id_removal_2 = 2;
    for i in 0..7 {
        client.vote_on_proposal(&members.get(i).unwrap(), &prop_id_removal_2, &true);
    }
    env.ledger().set_timestamp(200000);
    client.execute_proposal(&prop_id_removal_2);
    let prop_removal_2 = client.get_proposal(&prop_id_removal_2).unwrap();
    assert_eq!(prop_removal_2.status, ProposalStatus::Executed); // 70% >= 67%
}

#[test]
fn test_proposal_respects_quorum_at_creation() {
    let (env, client, admin, token_admin, members) = setup_with_members(10);

    client.init(&admin, &members, &100, &token_admin, &3600, &RoscaConfig {
        strategy: PayoutStrategy::RoundRobin,
        custom_order: None, penalty_amount: 0, exit_penalty_bps: 0, collective_goal: None, member_goals: None, fee_bps: 0, fee_recipient: None, max_defaults: 3,
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
        }, &None);

    let creator = members.get(0).unwrap();

    // 1. Create proposal with default 51% quorum
    client.create_proposal(&creator, &ProposalType::RuleChange, &soroban_sdk::String::from_str(&env, "rule"), &creator, &86400, &None);
    let prop_id = 0;

    // 2. Update quorum to 80%
    client.set_quorum_per_type(&admin, &ProposalType::RuleChange, &8000);

    // 3. Vote 6 times (60%)
    for i in 0..6 {
        client.vote_on_proposal(&members.get(i).unwrap(), &prop_id, &true);
    }

    env.ledger().set_timestamp(100000);
    client.execute_proposal(&prop_id);

    let prop = client.get_proposal(&prop_id).unwrap();
    assert_eq!(prop.status, ProposalStatus::Executed); // Still used 51% from creation time
}

#[test]
fn test_treasury_round_requires_quorum() {
    let (env, client, admin, token_admin, members) = setup_with_members(10);

    client.init(&admin, &members, &100, &token_admin, &3600, &RoscaConfig {
        strategy: PayoutStrategy::RoundRobin,
        custom_order: None, penalty_amount: 0, exit_penalty_bps: 0,
        collective_goal: None, member_goals: None, fee_bps: 0, fee_recipient: None,
        max_defaults: 3, grace_period_ledgers: 0, grace_period_seconds: 0,
        use_timestamp_schedule: false, round_duration_seconds: 0, max_members: None,
        skip_fee: 0, max_skips_per_cycle: 0, voting_mode: VotingMode::Equal,
        late_fee_bps: 0, auction_enabled: false, auction_window_ledgers: 0,
        randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,
    }, &None);

    let proposer = members.get(0).unwrap();
    let purpose = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

    // Set quorum for RuleChange to 51%
    client.set_quorum_per_type(&admin, &ProposalType::RuleChange, &5_100);

    client.propose_treasury_round(&proposer, &0, &purpose);

    // Single-member vote must NOT confirm in a 10-member group (1 < 51% of 10 = 5)
    client.vote_treasury_round(&proposer, &0, &true);
    let proposal = client.get_treasury_round_proposal(&0);
    assert!(!proposal.confirmed);

    // Vote 4 more (total 5 = 50%) — still not enough
    for i in 1..5 {
        client.vote_treasury_round(&members.get(i).unwrap(), &0, &true);
    }
    let proposal = client.get_treasury_round_proposal(&0);
    assert!(!proposal.confirmed);

    // 6th vote (60%) — quorum reached
    client.vote_treasury_round(&members.get(5).unwrap(), &0, &true);
    let proposal = client.get_treasury_round_proposal(&0);
    assert!(proposal.confirmed);
}

#[test]
fn test_treasury_round_vote_non_member_rejected() {
    let (env, client, admin, token_admin, members) = setup_with_members(3);

    client.init(&admin, &members, &100, &token_admin, &3600, &RoscaConfig {
        strategy: PayoutStrategy::RoundRobin,
        custom_order: None, penalty_amount: 0, exit_penalty_bps: 0,
        collective_goal: None, member_goals: None, fee_bps: 0, fee_recipient: None,
        max_defaults: 3, grace_period_ledgers: 0, grace_period_seconds: 0,
        use_timestamp_schedule: false, round_duration_seconds: 0, max_members: None,
        skip_fee: 0, max_skips_per_cycle: 0, voting_mode: VotingMode::Equal,
        late_fee_bps: 0, auction_enabled: false, auction_window_ledgers: 0,
        randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,
    }, &None);

    let proposer = members.get(0).unwrap();
    let outsider = Address::generate(&env);
    let purpose = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

    client.propose_treasury_round(&proposer, &0, &purpose);

    let err = client.try_vote_treasury_round(&outsider, &0, &true).unwrap_err().unwrap();
    assert_eq!(err, Error::NotAMember.into());
}
