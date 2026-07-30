//! Integration tests: token-whitelist + escrow + refund deployed together.
//!
//! Tests the full cross-contract refund lifecycle:
//!   whitelist a token → fund escrow → dispute → arbiter awards buyer → register
//!   refund record in the refund contract → verify the record.
//!
//! Also verifies that a non-whitelisted origin contract cannot register refunds.

use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token, Address, Env, Vec,
};

use ahjoor_escrow::{
    AhjoorEscrowContract, AhjoorEscrowContractClient, EscrowCreateRequest, RenewalConditionPolicy,
};
use ahjoor_refund::{
    AhjoorRefundContract, AhjoorRefundContractClient, RefundInitConfig, RefundStatus,
};
use ahjoor_token_whitelist::{TokenWhitelistContract, TokenWhitelistContractClient};

// ---------------------------------------------------------------------------
// Shared harness
// ---------------------------------------------------------------------------

struct CrossContractEnv<'a> {
    env: Env,
    admin: Address,
    token_addr: Address,
    token_sac: token::StellarAssetClient<'a>,
    token_client: token::Client<'a>,
    whitelist: TokenWhitelistContractClient<'a>,
    escrow: AhjoorEscrowContractClient<'a>,
    escrow_addr: Address,
    refund: AhjoorRefundContractClient<'a>,
}

impl<'a> CrossContractEnv<'a> {
    fn setup() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set(LedgerInfo {
            timestamp: 1_000_000,
            protocol_version: 23,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });

        let admin = Address::generate(&env);
        let token_admin_addr = Address::generate(&env);

        // Deploy a Stellar Asset Contract as the test token.
        let token_contract = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
        let token_addr = token_contract.address();
        let token_sac = token::StellarAssetClient::new(&env, &token_addr);
        let token_client = token::Client::new(&env, &token_addr);

        // Deploy and initialise the token-whitelist contract.
        let wl_id = env.register(TokenWhitelistContract, ());
        let whitelist = TokenWhitelistContractClient::new(&env, &wl_id);
        whitelist.initialize(&admin);
        // Add the test token to the global whitelist.
        whitelist.add_token(&admin, &token_addr);

        // Deploy and initialise the escrow contract, then point it at the whitelist.
        let escrow_id = env.register(AhjoorEscrowContract, ());
        let escrow = AhjoorEscrowContractClient::new(&env, &escrow_id);
        escrow.initialize(&admin);
        escrow.set_token_whitelist_contract(&admin, &wl_id);
        // Grant the escrow contract a permanent per-contract allowlist entry so that
        // is_token_allowed_for_contract(escrow_addr, token) returns true.
        whitelist.set_contract_token(&admin, &escrow_id, &token_addr, &None);

        // Deploy and initialise the refund contract (payment-contract address is not
        // exercised by the cross-contract path, so a dummy address is fine).
        let dummy_payment = Address::generate(&env);
        let refund_id = env.register(AhjoorRefundContract, ());
        let refund = AhjoorRefundContractClient::new(&env, &refund_id);
        refund.initialize(
            &admin,
            &dummy_payment,
            &0u64,
            &Some(RefundInitConfig {
                escrow_contract: None,
                refund_fee_bps: 0,
                fee_recipient: None,
                auto_reject_window_seconds: 0,
                appeal_window_seconds: 0,
                refund_tiers: None,
                refund_cooldown_seconds: 0,
                customer_cancel_window_seconds: 0,
            }),
        );
        // Authorise the escrow contract to register cross-contract refunds.
        refund.add_refund_origin_contract(&admin, &escrow_id);

        CrossContractEnv {
            env,
            admin,
            token_addr,
            token_sac,
            token_client,
            whitelist,
            escrow,
            escrow_addr: escrow_id,
            refund,
        }
    }

    fn make_escrow_request(&self, seller: &Address, arbiter: &Address, amount: i128) -> EscrowCreateRequest {
        EscrowCreateRequest {
            seller: seller.clone(),
            arbiter: arbiter.clone(),
            amount,
            token: self.token_addr.clone(),
            deadline: 1_000_000 + 86_400,
            metadata_hash: None,
            sellers: Vec::new(&self.env),
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
}

// ---------------------------------------------------------------------------
// Test 1: full lifecycle — whitelist → fund → dispute → 100% buyer award →
//          cross-contract refund registration → record verification
// ---------------------------------------------------------------------------

#[test]
fn test_full_lifecycle_escrow_dispute_refund() {
    let t = CrossContractEnv::setup();

    let buyer = Address::generate(&t.env);
    let seller = Address::generate(&t.env);
    let arbiter = Address::generate(&t.env);

    // Fund the buyer.
    t.token_sac.mint(&buyer, &10_000);

    // Create escrow: buyer deposits 2 000 tokens.
    let eid = t.escrow.create_escrow_v2(&buyer, &t.make_escrow_request(&seller, &arbiter, 2_000));

    assert_eq!(t.token_client.balance(&buyer), 8_000, "tokens should be held in escrow");

    // Buyer disputes the full amount (no undisputed portion released to seller).
    t.escrow.dispute_escrow(
        &buyer,
        &eid,
        &soroban_sdk::String::from_str(&t.env, "goods not received"),
        &2_000,
    );

    // Arbiter rules 100% in the buyer's favour.
    t.escrow.resolve_dispute(&arbiter, &eid, &100u32);

    // Buyer receives their funds back from the escrow.
    assert_eq!(t.token_client.balance(&buyer), 10_000, "buyer should recover all funds after arbiter ruling");

    // The escrow contract registers the refund in the refund contract, simulating
    // the cross-contract callback.  With mock_all_auths() the origin_contract.require_auth()
    // inside register_cross_contract_refund is satisfied automatically.
    let refund_id = t.refund.register_cross_contract_refund(
        &t.escrow_addr,
        &eid,
        &buyer,
        &seller,
        &t.token_addr,
        &2_000i128,
        &0u32,
    );

    // Verify the refund record.
    let rec = t.refund.get_refund(&refund_id);
    assert_eq!(rec.id, refund_id);
    assert_eq!(rec.customer, buyer);
    assert_eq!(rec.merchant, seller);
    assert_eq!(rec.amount, 2_000);
    assert_eq!(rec.token, t.token_addr);
    assert_eq!(rec.escrow_id, Some(eid));
    assert_eq!(rec.status, RefundStatus::CrossContractRefunded);
    assert_eq!(rec.origin_contract, Some(t.escrow_addr.clone()));
    assert_eq!(rec.reason_code, 0);

    // The record appears in the cross-contract queue.
    let (queue, total, _) = t.refund.get_cross_contract_refund_queue(&0u32, &50u32);
    assert_eq!(total, 1);
    let queued_rec = queue.get(0).unwrap();
    assert_eq!(queued_rec.id, refund_id);

    // Customer index is populated.
    let customer_refunds = t.refund.get_refunds_by_customer(&buyer, &10u32, &0u32);
    assert_eq!(customer_refunds.len(), 1);
    assert_eq!(customer_refunds.get(0).unwrap(), refund_id);
}

// ---------------------------------------------------------------------------
// Test 2: non-whitelisted contract cannot register cross-contract refunds
// ---------------------------------------------------------------------------

#[test]
fn test_non_whitelisted_origin_rejected() {
    let t = CrossContractEnv::setup();

    let intruder = Address::generate(&t.env);
    let buyer = Address::generate(&t.env);
    let seller = Address::generate(&t.env);

    // `intruder` is not in the cross-contract whitelist → must panic.
    let result = t.refund.try_register_cross_contract_refund(
        &intruder,
        &0u32,
        &buyer,
        &seller,
        &t.token_addr,
        &500i128,
        &0u32,
    );

    assert!(result.is_err(), "non-whitelisted origin contract must be rejected");
}

// ---------------------------------------------------------------------------
// Test 3: multiple escrows from the same pair each get independent refund records
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_escrows_produce_separate_refund_records() {
    let t = CrossContractEnv::setup();

    let buyer = Address::generate(&t.env);
    let seller = Address::generate(&t.env);
    let arbiter = Address::generate(&t.env);

    t.token_sac.mint(&buyer, &30_000);

    // First escrow: 1 000 tokens
    let eid1 = t.escrow.create_escrow_v2(&buyer, &t.make_escrow_request(&seller, &arbiter, 1_000));
    t.escrow.dispute_escrow(
        &buyer, &eid1, &soroban_sdk::String::from_str(&t.env, "d1"), &1_000,
    );
    t.escrow.resolve_dispute(&arbiter, &eid1, &100u32);

    let rid1 = t.refund.register_cross_contract_refund(
        &t.escrow_addr, &eid1, &buyer, &seller, &t.token_addr, &1_000i128, &0u32,
    );

    // Second escrow: 2 000 tokens
    let eid2 = t.escrow.create_escrow_v2(&buyer, &t.make_escrow_request(&seller, &arbiter, 2_000));
    t.escrow.dispute_escrow(
        &buyer, &eid2, &soroban_sdk::String::from_str(&t.env, "d2"), &2_000,
    );
    t.escrow.resolve_dispute(&arbiter, &eid2, &100u32);

    let rid2 = t.refund.register_cross_contract_refund(
        &t.escrow_addr, &eid2, &buyer, &seller, &t.token_addr, &2_000i128, &0u32,
    );

    assert_ne!(rid1, rid2, "each escrow must produce a distinct refund record");

    let rec1 = t.refund.get_refund(&rid1);
    let rec2 = t.refund.get_refund(&rid2);
    assert_eq!(rec1.amount, 1_000);
    assert_eq!(rec2.amount, 2_000);
    assert_eq!(rec1.escrow_id, Some(eid1));
    assert_eq!(rec2.escrow_id, Some(eid2));

    // Both records appear in the cross-contract queue.
    let (_, total, _) = t.refund.get_cross_contract_refund_queue(&0u32, &50u32);
    assert_eq!(total, 2);
}
