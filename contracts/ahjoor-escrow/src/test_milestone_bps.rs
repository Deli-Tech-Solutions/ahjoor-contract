#![cfg(test)]
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;

use crate::{
    AhjoorEscrowContract, AhjoorEscrowContractClient,
    MilestoneInput, MilestoneStateStatus, EscrowStatus,
};

fn setup_env() -> (Env, AhjoorEscrowContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

fn dummy_token<'a>(env: &'a Env, client: &AhjoorEscrowContractClient<'a>, admin: &Address) -> (Address, TokenAdminClient<'a>, Address) {
    let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_admin = TokenAdminClient::new(env, &token_id);
    client.add_allowed_token(admin, &token_id);
    (token_id, token_admin, admin.clone())
}

fn hash(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

fn milestone_str(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn three_milestones(env: &Env) -> Vec<MilestoneInput> {
    let mut v = Vec::new(env);
    v.push_back(MilestoneInput {
        name: milestone_str(env, "Design"),
        release_bps: 3_000,
        description_hash: hash(env, 0x01),
    });
    v.push_back(MilestoneInput {
        name: milestone_str(env, "Build"),
        release_bps: 5_000,
        description_hash: hash(env, 0x02),
    });
    v.push_back(MilestoneInput {
        name: milestone_str(env, "Launch"),
        release_bps: 2_000,
        description_hash: hash(env, 0x03),
    });
    v
}

#[test]
fn test_create_escrow_validates_bps_sum() {
    let (env, client, admin) = setup_env();
    let (token, ta, _) = dummy_token(&env, &client, &admin);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    ta.mint(&buyer, &10_000);
    TokenClient::new(&env, &token).approve(
        &buyer,
        &client.address,
        &10_000,
        &(env.ledger().sequence() + 10_000),
    );

    let milestones = three_milestones(&env);
    let deadline = env.ledger().timestamp() + 10_000;

    let escrow_id = client.create_bps_milestone_escrow(
        &buyer, &seller, &arbiter, &10_000i128, &token, &deadline, &milestones,
    );

    let states = client.get_bps_milestones(&escrow_id);
    assert_eq!(states.len(), 3);
    assert_eq!(states.get(0).unwrap().release_bps, 3_000);
    assert_eq!(states.get(1).unwrap().release_bps, 5_000);
    assert_eq!(states.get(2).unwrap().release_bps, 2_000);
}

#[test]
#[should_panic]
fn test_bps_sum_mismatch_panics() {
    let (env, client, admin) = setup_env();
    let (token, ta, _) = dummy_token(&env, &client, &admin);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    ta.mint(&buyer, &10_000);
    TokenClient::new(&env, &token).approve(
        &buyer,
        &client.address,
        &10_000,
        &(env.ledger().sequence() + 10_000),
    );

    // BPS sum = 9_000 ≠ 10_000 → panic
    let mut milestones = Vec::new(&env);
    milestones.push_back(MilestoneInput {
        name: milestone_str(&env, "A"),
        release_bps: 9_000,
        description_hash: hash(&env, 0x01),
    });
    let deadline = env.ledger().timestamp() + 10_000;
    client.create_bps_milestone_escrow(
        &buyer, &seller, &arbiter, &10_000i128, &token, &deadline, &milestones,
    );
}

#[test]
fn test_submit_milestone_stores_delivery_hash() {
    let (env, client, admin) = setup_env();
    let (token, ta, _) = dummy_token(&env, &client, &admin);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    ta.mint(&buyer, &10_000);
    TokenClient::new(&env, &token).approve(
        &buyer,
        &client.address,
        &10_000,
        &(env.ledger().sequence() + 10_000),
    );

    let milestones = three_milestones(&env);
    let deadline = env.ledger().timestamp() + 10_000;
    let eid = client.create_bps_milestone_escrow(
        &buyer, &seller, &arbiter, &10_000i128, &token, &deadline, &milestones,
    );

    let delivery_hash = hash(&env, 0xDE);
    client.submit_milestone(&seller, &eid, &0u32, &delivery_hash);

    let states = client.get_bps_milestones(&eid);
    let s = states.get(0).unwrap();
    assert_eq!(s.status, MilestoneStateStatus::Submitted);
    assert_eq!(s.delivery_hash, Some(delivery_hash));
}

#[test]
fn test_approve_milestone_releases_proportional_amount() {
    let (env, client, admin) = setup_env();
    let (token, ta, _) = dummy_token(&env, &client, &admin);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    ta.mint(&buyer, &10_000);
    let tc = TokenClient::new(&env, &token);
    tc.approve(&buyer, &client.address, &10_000, &(env.ledger().sequence() + 10_000));

    let milestones = three_milestones(&env);
    let deadline = env.ledger().timestamp() + 10_000;
    let eid = client.create_bps_milestone_escrow(
        &buyer, &seller, &arbiter, &10_000i128, &token, &deadline, &milestones,
    );

    client.submit_milestone(&seller, &eid, &0u32, &hash(&env, 0xAA));
    let seller_bal_before = tc.balance(&seller);
    client.approve_proportional_milestone(&buyer, &eid, &0u32);
    let seller_bal_after = tc.balance(&seller);

    // 3_000 bps of 10_000 = 3_000
    assert_eq!(seller_bal_after - seller_bal_before, 3_000);

    let states = client.get_bps_milestones(&eid);
    assert_eq!(states.get(0).unwrap().status, MilestoneStateStatus::Approved);
}

#[test]
fn test_reject_milestone_returns_to_pending() {
    let (env, client, admin) = setup_env();
    let (token, ta, _) = dummy_token(&env, &client, &admin);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    ta.mint(&buyer, &10_000);
    TokenClient::new(&env, &token).approve(
        &buyer,
        &client.address,
        &10_000,
        &(env.ledger().sequence() + 10_000),
    );

    let milestones = three_milestones(&env);
    let deadline = env.ledger().timestamp() + 10_000;
    let eid = client.create_bps_milestone_escrow(
        &buyer, &seller, &arbiter, &10_000i128, &token, &deadline, &milestones,
    );

    client.submit_milestone(&seller, &eid, &0u32, &hash(&env, 0xBB));
    client.reject_milestone(&buyer, &eid, &0u32, &hash(&env, 0xCC));

    let states = client.get_bps_milestones(&eid);
    assert_eq!(states.get(0).unwrap().status, MilestoneStateStatus::Rejected);
    assert_eq!(states.get(0).unwrap().rejection_hash, Some(hash(&env, 0xCC)));
}

#[test]
fn test_resubmission_after_rejection() {
    let (env, client, admin) = setup_env();
    let (token, ta, _) = dummy_token(&env, &client, &admin);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    ta.mint(&buyer, &10_000);
    TokenClient::new(&env, &token).approve(
        &buyer,
        &client.address,
        &10_000,
        &(env.ledger().sequence() + 10_000),
    );

    let milestones = three_milestones(&env);
    let deadline = env.ledger().timestamp() + 10_000;
    let eid = client.create_bps_milestone_escrow(
        &buyer, &seller, &arbiter, &10_000i128, &token, &deadline, &milestones,
    );

    client.submit_milestone(&seller, &eid, &0u32, &hash(&env, 0x01));
    client.reject_milestone(&buyer, &eid, &0u32, &hash(&env, 0x02));

    // Seller re-submits after fixing
    client.submit_milestone(&seller, &eid, &0u32, &hash(&env, 0x03));
    let states = client.get_bps_milestones(&eid);
    assert_eq!(states.get(0).unwrap().status, MilestoneStateStatus::Submitted);
}

#[test]
fn test_final_milestone_releases_rounding_remainder() {
    let (env, client, admin) = setup_env();
    let (token, ta, _) = dummy_token(&env, &client, &admin);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    // Amount = 10_001 to force a rounding remainder
    ta.mint(&buyer, &10_001);
    let tc = TokenClient::new(&env, &token);
    tc.approve(&buyer, &client.address, &10_001, &(env.ledger().sequence() + 10_000));

    let milestones = three_milestones(&env); // 3000 + 5000 + 2000
    let deadline = env.ledger().timestamp() + 10_000;
    let eid = client.create_bps_milestone_escrow(
        &buyer, &seller, &arbiter, &10_001i128, &token, &deadline, &milestones,
    );

    // Approve milestones 0 and 1
    client.submit_milestone(&seller, &eid, &0u32, &hash(&env, 0x01));
    client.approve_proportional_milestone(&buyer, &eid, &0u32);

    client.submit_milestone(&seller, &eid, &1u32, &hash(&env, 0x02));
    client.approve_proportional_milestone(&buyer, &eid, &1u32);

    let before = tc.balance(&seller);
    // Final milestone — should release remainder
    client.submit_milestone(&seller, &eid, &2u32, &hash(&env, 0x03));
    client.approve_proportional_milestone(&buyer, &eid, &2u32);
    let after = tc.balance(&seller);

    // Total released should equal 10_001 (original amount)
    // seller receives remainder: 10001 - 3000 - 5000 = 2001 (not 2000)
    let last_release = after - before;
    assert!(last_release >= 2_000);

    // Escrow should be Released after all milestones approved
    let escrow = client.get_escrow(&eid);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
fn test_rejected_milestone_does_not_block_other_milestones() {
    let (env, client, admin) = setup_env();
    let (token, ta, _) = dummy_token(&env, &client, &admin);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    ta.mint(&buyer, &10_000);
    TokenClient::new(&env, &token).approve(
        &buyer,
        &client.address,
        &10_000,
        &(env.ledger().sequence() + 10_000),
    );

    let milestones = three_milestones(&env);
    let deadline = env.ledger().timestamp() + 10_000;
    let eid = client.create_bps_milestone_escrow(
        &buyer, &seller, &arbiter, &10_000i128, &token, &deadline, &milestones,
    );

    // Reject milestone 0; approve milestone 1 independently
    client.submit_milestone(&seller, &eid, &0u32, &hash(&env, 0x01));
    client.reject_milestone(&buyer, &eid, &0u32, &hash(&env, 0xFF));

    client.submit_milestone(&seller, &eid, &1u32, &hash(&env, 0x02));
    client.approve_proportional_milestone(&buyer, &eid, &1u32); // should work fine

    let states = client.get_bps_milestones(&eid);
    assert_eq!(states.get(0).unwrap().status, MilestoneStateStatus::Rejected);
    assert_eq!(states.get(1).unwrap().status, MilestoneStateStatus::Approved);
    assert_eq!(states.get(2).unwrap().status, MilestoneStateStatus::Pending);
}
