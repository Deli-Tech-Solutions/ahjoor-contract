#![cfg(test)]
use super::*;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};

fn setup_freeze_test<'a>() -> (Env, AhjoorContractClient<'a>, Address, Address, soroban_sdk::Vec<Address>) {
    setup_freeze_test_with_round_duration(3600)
}

fn setup_freeze_test_with_round_duration<'a>(round_duration: u64) -> (Env, AhjoorContractClient<'a>, Address, Address, soroban_sdk::Vec<Address>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

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
        &token_admin,
        &round_duration,
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

    env.ledger().set_timestamp(100);

    (env, client, admin, token_admin, members)
}

fn reason_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

fn resolution_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[2u8; 32])
}

#[test]
fn test_freeze_blocks_contribute() {
    let (env, client, admin, token_admin, members) = setup_freeze_test();
    let member = members.get(0).unwrap();

    client.freeze_group(&admin, &0, &reason_hash(&env));

    let result = client.try_contribute(&member, &token_admin, &100);
    assert!(result.is_err());
}

#[test]
fn test_freeze_blocks_close_round() {
    let (env, client, admin, _token_admin, _members) = setup_freeze_test();

    client.freeze_group(&admin, &0, &reason_hash(&env));

    // Advance past deadline
    env.ledger().set_timestamp(100_000);
    let result = client.try_close_round();
    assert!(result.is_err());
}

#[test]
fn test_freeze_blocks_add_member() {
    let (env, client, admin, _token_admin, _members) = setup_freeze_test();
    let new_member = Address::generate(&env);

    client.freeze_group(&admin, &0, &reason_hash(&env));

    let result = client.try_add_member(&new_member);
    assert!(result.is_err());
}

#[test]
fn test_freeze_blocks_remove_member() {
    let (env, client, admin, _token_admin, members) = setup_freeze_test();
    let member = members.get(0).unwrap();

    client.freeze_group(&admin, &0, &reason_hash(&env));

    let result = client.try_remove_member(&member);
    assert!(result.is_err());
}

#[test]
fn test_read_queries_succeed_during_freeze() {
    let (env, client, admin, _token_admin, members) = setup_freeze_test();
    let member = members.get(0).unwrap();

    client.freeze_group(&admin, &0, &reason_hash(&env));

    // Read-only queries must not panic
    let _info = client.get_group_info();
    let _status = client.get_member_status(&member);
}

#[test]
fn test_unfreeze_restores_operations() {
    let (env, client, admin, token_admin, members) = setup_freeze_test();
    let member = members.get(0).unwrap();

    client.freeze_group(&admin, &0, &reason_hash(&env));
    client.unfreeze_group(&admin, &0, &resolution_hash(&env));

    // Contribute should succeed after unfreeze
    client.contribute(&member, &token_admin, &100);
}

#[test]
fn test_non_admin_cannot_freeze() {
    let (env, client, _admin, _token_admin, members) = setup_freeze_test();
    let non_admin = members.get(0).unwrap();

    let result = client.try_freeze_group(&non_admin, &0, &reason_hash(&env));
    assert!(result.is_err());
}

#[test]
fn test_freeze_log_appended() {
    let (env, client, admin, _token_admin, _members) = setup_freeze_test();

    client.freeze_group(&admin, &0, &reason_hash(&env));
    client.unfreeze_group(&admin, &0, &resolution_hash(&env));

    let log = client.get_freeze_log();
    assert_eq!(log.len(), 1);
    let record = log.get(0).unwrap();
    assert_eq!(record.reason_hash, reason_hash(&env));
    assert!(record.unfrozen_at_ledger.is_some());
    assert_eq!(record.resolution_hash, Some(resolution_hash(&env)));
}

#[test]
fn test_member_freeze_proposal_executes_and_freezes_group() {
    // Member-freeze proposals require the mandatory 24h voting window to
    // elapse before execution, which alone outlasts the default 3600s round
    // used by the other freeze tests — so this test needs a round long
    // enough that the contribution window is still open once we get there.
    let (env, client, admin, _token_admin, members) = setup_freeze_test_with_round_duration(200_000);
    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();
    let member3 = members.get(2).unwrap();
    let member_reason = BytesN::from_array(&env, &[7u8; 32]);

    client.propose_member_freeze(&member1, &member_reason);
    let proposal = client.get_proposal(&0).unwrap();
    assert_eq!(proposal.proposal_type, ProposalType::MemberFreeze);
    assert_eq!(proposal.required_quorum, 6_700);

    client.vote_on_proposal(&member1, &0, &true);
    client.vote_on_proposal(&member2, &0, &true);
    client.vote_on_proposal(&member3, &0, &true);

    env.ledger().set_timestamp(100_000);
    client.execute_proposal(&0);

    // Group should now be frozen until admin unfreezes
    let contribute_res = client.try_contribute(&member1, &_token_admin, &100);
    assert!(contribute_res.is_err());

    // Existing admin unfreeze path still works
    client.unfreeze_group(&admin, &0, &resolution_hash(&env));
    client.contribute(&member1, &_token_admin, &100);

    let log = client.get_freeze_log();
    assert!(log.len() >= 1);
    let record = log.get(log.len() - 1).unwrap();
    assert_eq!(record.reason_hash, member_reason);
}

#[test]
fn test_non_member_cannot_propose_member_freeze() {
    let (env, client, _admin, _token_admin, _members) = setup_freeze_test();
    let outsider = Address::generate(&env);

    let result = client.try_propose_member_freeze(&outsider, &reason_hash(&env));
    assert!(result.is_err());
}
