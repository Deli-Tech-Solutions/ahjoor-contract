//! Integration tests: token suspension mid-flow.
//!
//! Deploys token-whitelist + escrow + rosca together and verifies that after the
//! admin suspends a token in the whitelist contract, every dependent contract
//! correctly rejects further operations that transfer that token.
//!
//! Flow:
//!   1. Whitelist token → contracts accept it.
//!   2. Fund escrow / start ROSCA round → succeeds.
//!   3. Admin suspends the token in the whitelist contract.
//!   4. New escrow creation with that token → rejected.
//!   5. ROSCA contribution with that token → rejected.

use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token, Address, BytesN, Env, Vec,
};

use ahjoor_escrow::{
    AhjoorEscrowContract, AhjoorEscrowContractClient, EscrowCreateRequest, RenewalConditionPolicy,
};
use ahjoor_rosca::{AhjoorContract, AhjoorContractClient, PayoutStrategy, RoscaConfig, VotingMode};
use ahjoor_token_whitelist::{TokenWhitelistContract, TokenWhitelistContractClient};

// ---------------------------------------------------------------------------
// Shared setup helpers
// ---------------------------------------------------------------------------

fn make_ledger_info() -> LedgerInfo {
    LedgerInfo {
        timestamp: 1_000_000,
        protocol_version: 23,
        sequence_number: 100,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 6_312_000,
    }
}

fn reason_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xdeu8; 32])
}

fn make_escrow_request<'a>(
    env: &Env,
    seller: &Address,
    arbiter: &Address,
    token: &Address,
    amount: i128,
) -> EscrowCreateRequest {
    EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount,
        token: token.clone(),
        deadline: 1_000_000 + 86_400,
        metadata_hash: None,
        sellers: Vec::new(env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: None,
        dispute_default_winner: None,
        auto_renew_max_renewals: None,
        auto_renew_interval_ledgers: None,
        renewal_condition_policy: RenewalConditionPolicy::Reset,
    }
}

fn make_rosca_config() -> RoscaConfig {
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
        round_duration_seconds: 86_400,
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
    }
}

// ---------------------------------------------------------------------------
// Test 1: suspended token blocks new escrow creation
// ---------------------------------------------------------------------------

#[test]
fn test_suspended_token_blocks_escrow_creation() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(make_ledger_info());

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    // Deploy and wire the token.
    let token_admin_addr = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_addr = token_contract.address();
    let token_sac = token::StellarAssetClient::new(&env, &token_addr);
    token_sac.mint(&buyer, &20_000);

    // Deploy token-whitelist.
    let wl_id = env.register(TokenWhitelistContract, ());
    let whitelist = TokenWhitelistContractClient::new(&env, &wl_id);
    whitelist.initialize(&admin);
    whitelist.add_token(&admin, &token_addr);

    // Deploy escrow and connect it to the whitelist.
    let escrow_id = env.register(AhjoorEscrowContract, ());
    let escrow = AhjoorEscrowContractClient::new(&env, &escrow_id);
    escrow.initialize(&admin);
    escrow.set_token_whitelist_contract(&admin, &wl_id);
    // Grant the escrow a permanent per-contract allowlist entry.
    whitelist.set_contract_token(&admin, &escrow_id, &token_addr, &None);

    // First escrow creation succeeds while token is active.
    let req = make_escrow_request(&env, &seller, &arbiter, &token_addr, 1_000);
    escrow.create_escrow_v2(&buyer, &req);

    // Admin suspends the token for 10 000 ledgers.
    whitelist.suspend_token_timed(&admin, &token_addr, &10_000u32, &reason_hash(&env));

    // Verify the suspension is recorded.
    let suspension = whitelist.get_token_suspension(&token_addr);
    assert!(suspension.is_some(), "token should now have an active suspension");

    // A second escrow creation with the same token must now be rejected.
    let result = escrow.try_create_escrow_v2(&buyer, &req);
    assert!(
        result.is_err(),
        "escrow creation must fail when token is suspended"
    );
}

// ---------------------------------------------------------------------------
// Test 2: suspended token blocks ROSCA contributions mid-round
// ---------------------------------------------------------------------------

#[test]
fn test_suspended_token_blocks_rosca_contribution() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(make_ledger_info());

    let admin = Address::generate(&env);
    let contribution: i128 = 500;

    // Deploy and wire the token.
    let token_admin_addr = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_addr = token_contract.address();
    let token_sac = token::StellarAssetClient::new(&env, &token_addr);

    // Deploy token-whitelist.
    let wl_id = env.register(TokenWhitelistContract, ());
    let whitelist = TokenWhitelistContractClient::new(&env, &wl_id);
    whitelist.initialize(&admin);
    whitelist.add_token(&admin, &token_addr);

    // Deploy ROSCA; initialise first (require_admin checks init state before
    // set_token_whitelist_contract can be called).
    let rosca_id = env.register(AhjoorContract, ());
    let rosca = AhjoorContractClient::new(&env, &rosca_id);

    // Generate 3 members and fund them.
    let mut members = Vec::new(&env);
    for _ in 0..3 {
        let m = Address::generate(&env);
        token_sac.mint(&m, &(contribution * 10));
        members.push_back(m);
    }

    // Initialise the ROSCA — whitelist not yet wired, so require_token_allowed
    // passes trivially (no whitelist contract set means all tokens are allowed).
    rosca.init(
        &admin,
        &members,
        &contribution,
        &token_addr,
        &86_400u64,
        &make_rosca_config(),
        &None,
    );

    // Now wire the whitelist contract so subsequent operations check it.
    rosca.set_token_whitelist_contract(&admin, &wl_id);

    // One member contributes successfully before suspension.
    let member0 = members.get(0).unwrap();
    rosca.contribute(&member0, &token_addr, &contribution);

    // Admin suspends the token.
    whitelist.suspend_token_timed(&admin, &token_addr, &10_000u32, &reason_hash(&env));

    // Any further contribution with the suspended token must be rejected.
    let member1 = members.get(1).unwrap();
    let result = rosca.try_contribute(&member1, &token_addr, &contribution);
    assert!(
        result.is_err(),
        "ROSCA contribution must fail when token is suspended"
    );
}

// ---------------------------------------------------------------------------
// Test 3: lifting suspension restores normal operation for both contracts
// ---------------------------------------------------------------------------

#[test]
fn test_lifting_suspension_restores_operations() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(make_ledger_info());

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    // Deploy token.
    let token_admin_addr = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_addr = token_contract.address();
    let token_sac = token::StellarAssetClient::new(&env, &token_addr);
    token_sac.mint(&buyer, &20_000);

    // Deploy whitelist.
    let wl_id = env.register(TokenWhitelistContract, ());
    let whitelist = TokenWhitelistContractClient::new(&env, &wl_id);
    whitelist.initialize(&admin);
    whitelist.add_token(&admin, &token_addr);

    // Deploy escrow.
    let escrow_id = env.register(AhjoorEscrowContract, ());
    let escrow = AhjoorEscrowContractClient::new(&env, &escrow_id);
    escrow.initialize(&admin);
    escrow.set_token_whitelist_contract(&admin, &wl_id);
    whitelist.set_contract_token(&admin, &escrow_id, &token_addr, &None);

    let req = make_escrow_request(&env, &seller, &arbiter, &token_addr, 1_000);

    // Escrow creation succeeds initially.
    escrow.create_escrow_v2(&buyer, &req);

    // Suspend the token.
    whitelist.suspend_token_timed(&admin, &token_addr, &10_000u32, &reason_hash(&env));

    // Escrow creation now fails.
    let mid_suspension_result = escrow.try_create_escrow_v2(&buyer, &req);
    assert!(mid_suspension_result.is_err(), "must fail during suspension");

    // Admin lifts the suspension early.
    whitelist.lift_token_suspension(&admin, &token_addr);

    // No active suspension should exist after lifting.
    let post_lift = whitelist.get_token_suspension(&token_addr);
    assert!(post_lift.is_none(), "suspension should be gone after lifting");

    // Escrow creation succeeds again.
    let post_lift_result = escrow.try_create_escrow_v2(&buyer, &req);
    assert!(
        post_lift_result.is_ok(),
        "escrow creation must succeed after suspension is lifted"
    );
}

// ---------------------------------------------------------------------------
// Test 4: existing escrow can still complete after token is suspended
//         (release_escrow does not re-validate the token whitelist)
// ---------------------------------------------------------------------------

#[test]
fn test_existing_escrow_completes_after_token_suspended() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(make_ledger_info());

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    // Deploy token.
    let token_admin_addr = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_addr = token_contract.address();
    let token_sac = token::StellarAssetClient::new(&env, &token_addr);
    let token_client = token::Client::new(&env, &token_addr);
    token_sac.mint(&buyer, &10_000);

    // Deploy whitelist + escrow.
    let wl_id = env.register(TokenWhitelistContract, ());
    let whitelist = TokenWhitelistContractClient::new(&env, &wl_id);
    whitelist.initialize(&admin);
    whitelist.add_token(&admin, &token_addr);

    let escrow_id = env.register(AhjoorEscrowContract, ());
    let escrow = AhjoorEscrowContractClient::new(&env, &escrow_id);
    escrow.initialize(&admin);
    escrow.set_token_whitelist_contract(&admin, &wl_id);
    whitelist.set_contract_token(&admin, &escrow_id, &token_addr, &None);

    // Create escrow while token is active.
    let req = make_escrow_request(&env, &seller, &arbiter, &token_addr, 3_000);
    let eid = escrow.create_escrow_v2(&buyer, &req);

    // Suspend the token.
    whitelist.suspend_token_timed(&admin, &token_addr, &10_000u32, &reason_hash(&env));

    // The existing escrow should still be releasable — the suspension only gates new deposits.
    let seller_balance_before = token_client.balance(&seller);
    escrow.release_escrow(&buyer, &eid);

    let seller_balance_after = token_client.balance(&seller);
    assert_eq!(
        seller_balance_after - seller_balance_before,
        3_000,
        "seller should receive funds from an escrow opened before suspension"
    );
}
