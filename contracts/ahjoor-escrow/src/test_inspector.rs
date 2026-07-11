#![cfg(test)]

use crate::{AhjoorEscrowContract, AhjoorEscrowContractClient, EscrowStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::Client as TokenClient,
    token::StellarAssetClient as TokenAdminClient,
    Address, BytesN, Env, String, Vec,
};

fn setup_test_env() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    AhjoorEscrowContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);
    token_admin_client.mint(&buyer, &10_000);

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.add_allowed_token(&admin, &token_addr);

    (env, admin, buyer, seller, arbiter, token_addr, client)
}

#[test]
fn test_inspector_approval_enables_release() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    // Seller marks work complete → AwaitingInspection
    client.seller_mark_complete(&seller, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::AwaitingInspection);

    // Submit inspection result (approved)
    let report_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
    client.submit_inspection_result(&inspector, &escrow_id, &true, &report_hash);

    // Verify status changed to InspectionPassed
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::InspectionPassed);

    // Now buyer can release
    client.release_escrow(&buyer, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
fn test_inspector_rejection_blocks_release() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    // Seller marks work complete → AwaitingInspection
    client.seller_mark_complete(&seller, &escrow_id);

    // Submit inspection result (rejected)
    let report_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
    client.submit_inspection_result(&inspector, &escrow_id, &false, &report_hash);

    // Verify status changed to InspectionFailed
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::InspectionFailed);

    // Buyer cannot release
    let result = client.try_release_escrow(&buyer, &escrow_id);
    assert!(result.is_err());
}

// ── #357: Inspector Reputation Scoring Tests ─────────────────────────────────

fn make_request(env: &soroban_sdk::Env, seller: &Address, arbiter: &Address, token: &Address, amount: i128) -> crate::EscrowCreateRequest {
    crate::EscrowCreateRequest {
        seller: seller.clone(), arbiter: arbiter.clone(), amount,
        token: token.clone(), deadline: env.ledger().timestamp() + 86400,
        metadata_hash: None, sellers: soroban_sdk::Vec::new(env), auto_renew: false,
        renewal_count: 0, buyer_inactivity_secs: 0, min_lock_until: None,
        release_base: None, release_quote: None, release_comparison: None,
        release_threshold_price: None, arbiter_fee_bps: None, dispute_default_winner: None,
        auto_renew_max_renewals: None,

        auto_renew_interval_ledgers: None,
    }
}

#[test]
fn test_inspector_score_initialized_on_first_ruling() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    // No score yet → neutral (0, 0, 10_000)
    let (total, correct, accuracy) = client.get_inspector_score(&inspector);
    assert_eq!(total, 0);
    assert_eq!(correct, 0);
    assert_eq!(accuracy, 10_000);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let eid = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
    client.dispute_escrow(&buyer, &eid, &String::from_str(&env, "test"), &1000);
    client.resolve_dispute(&arbiter, &eid, &50);

    // First ruling: initialized to neutral (1, 1, 10_000)
    let (total, correct, accuracy) = client.get_inspector_score(&inspector);
    assert_eq!(total, 1);
    assert_eq!(correct, 1);
    assert_eq!(accuracy, 10_000);
}

#[test]
fn test_inspector_score_increases_on_ruling() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    // Mint extra tokens for buyer
    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &5_000);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let e1 = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
    let e2 = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    client.dispute_escrow(&buyer, &e1, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &e1, &0);

    client.dispute_escrow(&buyer, &e2, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &e2, &0);

    // Two rulings: first initializes to (1,1), second increments to (2,2)
    let (total, correct, _accuracy) = client.get_inspector_score(&inspector);
    assert_eq!(total, 2);
    assert_eq!(correct, 2);
}

#[test]
fn test_inspector_score_decreases_on_appeal() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let eid = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
    client.dispute_escrow(&buyer, &eid, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &eid, &0);

    // Score after ruling: (1, 1)
    let (total_before, correct_before, _) = client.get_inspector_score(&inspector);
    assert_eq!(total_before, 1);
    assert_eq!(correct_before, 1);

    // Admin appeals → correct_rulings decremented
    client.appeal_inspector_ruling(&admin, &eid);

    let (total_after, correct_after, accuracy_after) = client.get_inspector_score(&inspector);
    assert_eq!(total_after, 1);
    assert_eq!(correct_after, 0);
    assert_eq!(accuracy_after, 0);
}

#[test]
fn test_low_score_inspector_blocked_from_high_value_escrow() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &100_000);

    // Create ruling + appeal → inspector score = 0/1 = 0 bps
    let req = make_request(&env, &seller, &arbiter, &token, 500);
    let e1 = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
    client.dispute_escrow(&buyer, &e1, &String::from_str(&env, "d"), &500);
    client.resolve_dispute(&arbiter, &e1, &0);
    client.appeal_inspector_ruling(&admin, &e1);

    // Set threshold: min 5000 bps for escrows above 1000
    client.set_inspector_score_threshold(&admin, &5_000u32, &1_000i128);

    // High-value escrow with low-score inspector must be rejected
    let hv_req = make_request(&env, &seller, &arbiter, &token, 5_000);
    let result = client.try_create_escrow_with_inspector(&buyer, &hv_req, &Some(inspector.clone()));
    assert!(result.is_err());
}

#[test]
fn test_replaced_inspector_score_penalized() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let old_inspector = Address::generate(&env);
    let new_inspector = Address::generate(&env);

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &5_000);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let first_escrow = client.create_escrow_with_inspector(&buyer, &req, &Some(old_inspector.clone()));
    client.dispute_escrow(&buyer, &first_escrow, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &first_escrow, &0);

    let (total_before, correct_before, accuracy_before) = client.get_inspector_score(&old_inspector);
    assert_eq!((total_before, correct_before, accuracy_before), (1, 1, 10_000));

    let replacement_escrow = client.create_escrow_with_inspector(&buyer, &req, &Some(old_inspector.clone()));
    client.replace_inspector(&buyer, &replacement_escrow, &new_inspector);
    client.replace_inspector(&seller, &replacement_escrow, &new_inspector);

    let (total_after, correct_after, accuracy_after) = client.get_inspector_score(&old_inspector);
    assert_eq!(total_after, 2);
    assert_eq!(correct_after, 1);
    assert!(accuracy_after < accuracy_before);
    assert_eq!(accuracy_after, 5_000);

    let (new_total, new_correct, new_accuracy) = client.get_inspector_score(&new_inspector);
    assert_eq!((new_total, new_correct, new_accuracy), (0, 0, 10_000));
}
