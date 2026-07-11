#![cfg(test)]
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env, Vec};
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::token::Client as TokenClient;

use crate::{
    AhjoorContract, AhjoorContractClient, RoscaConfig, PayoutStrategy, VotingMode,
    GroupStatus, SplitProposalStatus,
};

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

fn dummy_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xABu8; 32])
}

fn setup_split_rosca<'a>(
    env: &'a Env,
    members: &[Address],
) -> (AhjoorContractClient<'a>, Address, Address) {
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
    for m in members.iter() {
        token_admin.mint(m, &100_000);
        token_client.approve(m, &contract_id, &100_000, &(env.ledger().sequence() + 10_000));
    }

    let config = make_config(env);
    client.init(&admin, &members_vec, &1_000i128, &token_addr, &1000u64, &config, &None);
    (client, admin, token_addr)
}

#[test]
fn test_propose_group_split_stores_proposal() {
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    let m4 = Address::generate(&env);
    let (client, admin, _token) = setup_split_rosca(&env, &[m1.clone(), m2.clone(), m3.clone(), m4.clone()]);

    let a_members = Vec::from_slice(&env, &[m1.clone(), m2.clone()]);
    let b_members = Vec::from_slice(&env, &[m3.clone(), m4.clone()]);

    let proposal_id = client.propose_group_split(&admin, &0u32, &a_members, &b_members, &dummy_hash(&env));
    assert_eq!(proposal_id, 1u32);

    let proposal = client.get_split_proposal(&proposal_id);
    assert_eq!(proposal.status, SplitProposalStatus::Pending);
    assert_eq!(proposal.group_a_members.len(), 2);
    assert_eq!(proposal.group_b_members.len(), 2);
}

#[test]
#[should_panic]
fn test_propose_split_invalid_member_assignment_panics() {
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let (client, admin, _token) = setup_split_rosca(&env, &[m1.clone(), m2.clone()]);

    // m1 appears in both lists — invalid
    let a_members = Vec::from_slice(&env, &[m1.clone()]);
    let b_members = Vec::from_slice(&env, &[m1.clone(), m2.clone()]);
    client.propose_group_split(&admin, &0u32, &a_members, &b_members, &dummy_hash(&env));
}

#[test]
fn test_confirm_split_participation() {
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let (client, admin, _token) = setup_split_rosca(&env, &[m1.clone(), m2.clone()]);

    let a_members = Vec::from_slice(&env, &[m1.clone()]);
    let b_members = Vec::from_slice(&env, &[m2.clone()]);
    let proposal_id = client.propose_group_split(&admin, &0u32, &a_members, &b_members, &dummy_hash(&env));

    client.confirm_split_participation(&m1, &0u32, &proposal_id);
    client.confirm_split_participation(&m2, &0u32, &proposal_id);

    let proposal = client.get_split_proposal(&proposal_id);
    assert_eq!(proposal.confirmations.len(), 2);
}

#[test]
#[should_panic]
fn test_double_confirmation_panics() {
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let (client, admin, _token) = setup_split_rosca(&env, &[m1.clone(), m2.clone()]);

    let a_members = Vec::from_slice(&env, &[m1.clone()]);
    let b_members = Vec::from_slice(&env, &[m2.clone()]);
    let proposal_id = client.propose_group_split(&admin, &0u32, &a_members, &b_members, &dummy_hash(&env));

    client.confirm_split_participation(&m1, &0u32, &proposal_id);
    // Second confirmation from same member → panic
    client.confirm_split_participation(&m1, &0u32, &proposal_id);
}

#[test]
fn test_execute_group_split_marks_source_as_split() {
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let (client, admin, _token) = setup_split_rosca(&env, &[m1.clone(), m2.clone()]);

    let a_members = Vec::from_slice(&env, &[m1.clone()]);
    let b_members = Vec::from_slice(&env, &[m2.clone()]);
    let proposal_id = client.propose_group_split(&admin, &0u32, &a_members, &b_members, &dummy_hash(&env));

    client.confirm_split_participation(&m1, &0u32, &proposal_id);
    client.confirm_split_participation(&m2, &0u32, &proposal_id);
    client.execute_group_split(&admin, &0u32, &proposal_id);

    let proposal = client.get_split_proposal(&proposal_id);
    assert_eq!(proposal.status, SplitProposalStatus::Executed);
}

#[test]
#[should_panic]
fn test_operations_blocked_on_split_group() {
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let (client, admin, token) = setup_split_rosca(&env, &[m1.clone(), m2.clone()]);

    let a_members = Vec::from_slice(&env, &[m1.clone()]);
    let b_members = Vec::from_slice(&env, &[m2.clone()]);
    let proposal_id = client.propose_group_split(&admin, &0u32, &a_members, &b_members, &dummy_hash(&env));

    client.confirm_split_participation(&m1, &0u32, &proposal_id);
    client.confirm_split_participation(&m2, &0u32, &proposal_id);
    client.execute_group_split(&admin, &0u32, &proposal_id);

    // Attempting another split on an already-split group → panic
    let a2 = Vec::from_slice(&env, &[m1.clone()]);
    let b2 = Vec::from_slice(&env, &[m2.clone()]);
    client.propose_group_split(&admin, &0u32, &a2, &b2, &dummy_hash(&env));
}

// ── #400: complete_merge must require accepted == true ────────────────────────

/// complete_merge on a proposal with accepted=false must error with MigrationNotApproved.
#[test]
#[should_panic]
fn test_merge_requires_acceptance() {
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let (client, admin, _token) = setup_split_rosca(&env, &[m1.clone(), m2.clone()]);

    // propose_merge creates proposal with accepted=false
    let proposal_id = client.propose_merge(&admin, &99u32);

    // complete_merge WITHOUT calling accept_merge first → must panic (MigrationNotApproved)
    let new_members: Vec<Address> = Vec::new(&env);
    client.complete_merge(&admin, &proposal_id, &new_members);
}

/// complete_merge called twice on the same accepted proposal must error on the second call.
#[test]
#[should_panic]
fn test_complete_merge_double_execution_blocked() {
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let (client, admin, _token) = setup_split_rosca(&env, &[m1.clone(), m2.clone()]);

    let proposal_id = client.propose_merge(&admin, &99u32);
    client.accept_merge(&admin, &proposal_id);

    let new_members: Vec<Address> = Vec::new(&env);
    client.complete_merge(&admin, &proposal_id, &new_members);
    // Second call: GroupStatus is now Merged → must panic
    client.complete_merge(&admin, &proposal_id, &new_members);
}

/// Successful merge sets GroupStatus::Merged on the source group.
#[test]
fn test_complete_merge_sets_status_merged() {
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let (client, admin, _token) = setup_split_rosca(&env, &[m1.clone(), m2.clone()]);

    let proposal_id = client.propose_merge(&admin, &99u32);
    client.accept_merge(&admin, &proposal_id);

    let new_members: Vec<Address> = Vec::new(&env);
    client.complete_merge(&admin, &proposal_id, &new_members);

    assert_eq!(client.get_group_status(), GroupStatus::Merged);
}

#[test]
#[should_panic]
fn test_confirmation_window_enforced() {
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let (client, admin, _token) = setup_split_rosca(&env, &[m1.clone(), m2.clone()]);

    // Set a very short confirmation window
    client.set_split_confirmation_window(&admin, &1u32);

    let a_members = Vec::from_slice(&env, &[m1.clone()]);
    let b_members = Vec::from_slice(&env, &[m2.clone()]);
    let proposal_id = client.propose_group_split(&admin, &0u32, &a_members, &b_members, &dummy_hash(&env));

    // Advance ledger past window
    env.ledger().set_sequence_number(env.ledger().sequence() + 100);

    // Confirmation after window → panic
    client.confirm_split_participation(&m1, &0u32, &proposal_id);
}

// ── #401 acceptance criteria ─────────────────────────────────────────────────

/// confirm_split after expiry_ledger must return SplitConfirmationWindowClosed (#401).
#[test]
fn test_split_confirmation_rejects_after_expiry() {
    use crate::errors::ExtError;
    let env = Env::default();
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let (client, admin, _token) = setup_split_rosca(&env, &[m1.clone(), m2.clone()]);

    // Window of exactly 10 ledgers
    client.set_split_confirmation_window(&admin, &10u32);

    let a_members = Vec::from_slice(&env, &[m1.clone()]);
    let b_members = Vec::from_slice(&env, &[m2.clone()]);
    let proposal_id =
        client.propose_group_split(&admin, &0u32, &a_members, &b_members, &dummy_hash(&env));

    // Still within the window — confirmation must succeed
    client.confirm_split_participation(&m1, &0u32, &proposal_id);

    // Advance past expiry
    env.ledger().set_sequence_number(env.ledger().sequence() + 100);

    // Confirmation after window must return SplitConfirmationWindowClosed
    let err = client
        .try_confirm_split_participation(&m2, &0u32, &proposal_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError::SplitConfirmationWindowClosed.into());

    // expire_split_proposal must mark the proposal Expired
    client.expire_split_proposal(&proposal_id);
    let proposal = client.get_split_proposal(&proposal_id);
    assert_eq!(proposal.status, SplitProposalStatus::Expired);

    // execute_group_split on an expired proposal must return SplitProposalNotFound
    let exec_err = client
        .try_execute_group_split(&admin, &0u32, &proposal_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(exec_err, ExtError::SplitProposalNotFound.into());
}
