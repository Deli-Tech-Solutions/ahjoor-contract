#![cfg(test)]

//! #375: Tests for the commit-reveal sealed-bid slot auction with a minimum
//! reserve.
//!
//! Covers the acceptance criteria:
//! - Bids revealed before the reveal phase opens are rejected.
//! - Bids that do not match the committed hash are rejected during reveal.
//! - Unrevealed commits are forfeited (no refund of the deposit).
//! - The winning bid must exceed the minimum reserve, otherwise the slot is
//!   left unallocated.
//! - Valid reveal, invalid reveal, no-reserve-met, and sniping prevention.

use crate::{
    AhjoorContract, AhjoorContractClient, PayoutStrategy, RoscaConfig, VotingMode,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Bytes, BytesN, Env, Vec,
};
use soroban_sdk::xdr::ToXdr;

const COMMIT_DURATION: u64 = 500;
const REVEAL_DURATION: u64 = 500;

struct Harness<'a> {
    env: Env,
    client: AhjoorContractClient<'a>,
    token: token::Client<'a>,
    token_addr: Address,
    contract_id: Address,
    admin: Address,
    members: Vec<Address>,
}

fn base_config() -> RoscaConfig {
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

/// Set up a 3-member group, fund each member, and configure + open a sealed
/// auction for round 0 with the given minimum reserve. Time starts at 1000
/// (inside the commit phase).
fn setup<'a>(min_reserve: i128) -> Harness<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_sac = token::StellarAssetClient::new(
        &env,
        &env.register_stellar_asset_contract_v2(token_admin.clone())
            .address(),
    );
    let token_addr = token_sac.address.clone();
    let token = token::Client::new(&env, &token_addr);

    let mut members: Vec<Address> = Vec::new(&env);
    for _ in 0..3 {
        let m = Address::generate(&env);
        token_sac.mint(&m, &10_000);
        members.push_back(m);
    }

    env.ledger().set_timestamp(1_000);
    client.init(
        &admin,
        &members,
        &100,
        &token_addr,
        &3_600,
        &base_config(),
        &None,
    );

    client.configure_sealed_slot_auction(&admin, &COMMIT_DURATION, &REVEAL_DURATION, &min_reserve);
    client.open_sealed_slot_auction(&admin, &0);

    Harness {
        env,
        client,
        token,
        token_addr,
        contract_id,
        admin,
        members,
    }
}

/// Compute the commitment hash exactly as the contract does:
/// sha256(bidder.to_xdr() || bid_amount.to_be_bytes() || salt). Binding the
/// bidder defeats commit-copying / reveal front-running.
fn commit_hash(env: &Env, bidder: &Address, bid_amount: i128, salt: &BytesN<32>) -> BytesN<32> {
    let mut pre = Bytes::new(env);
    pre.append(&bidder.clone().to_xdr(env));
    pre.extend_from_array(&bid_amount.to_be_bytes());
    pre.extend_from_array(&salt.to_array());
    env.crypto().sha256(&pre).into()
}

#[test]
fn test_sealed_auction_highest_valid_bid_wins() {
    let hx = setup(100);
    let m0 = hx.members.get(0).unwrap();
    let m1 = hx.members.get(1).unwrap();
    let m2 = hx.members.get(2).unwrap();

    let salt1 = BytesN::from_array(&hx.env, &[7u8; 32]);
    let salt2 = BytesN::from_array(&hx.env, &[9u8; 32]);

    // ── Commit phase ── bids stay hidden; deposit is the bid upper bound.
    hx.client
        .commit_slot_bid(&m1, &commit_hash(&hx.env, &m1,300, &salt1), &300);
    hx.client
        .commit_slot_bid(&m2, &commit_hash(&hx.env, &m2,500, &salt2), &500);
    assert_eq!(hx.token.balance(&hx.contract_id), 800);

    // ── Reveal phase ──
    hx.env.ledger().set_timestamp(1_000 + COMMIT_DURATION + 1);
    hx.client.reveal_slot_bid(&m1, &0, &300, &salt1);
    hx.client.reveal_slot_bid(&m2, &0, &500, &salt2);
    assert_eq!(hx.client.get_sealed_revealed_bids(&0).len(), 2);

    // ── Settlement ── m2 (500) beats m1 (300) and the reserve (100).
    hx.env
        .ledger()
        .set_timestamp(1_000 + COMMIT_DURATION + REVEAL_DURATION + 1);
    hx.client.settle_sealed_slot_auction();

    // Winner m2 paid its full 500 (deposit 500, refund 0).
    assert_eq!(hx.token.balance(&m2), 10_000 - 500);
    // The 500 is split between the two non-winning members (250 each); m1 is
    // also refunded its 300 losing deposit.
    assert_eq!(hx.token.balance(&m1), 10_000 + 250);
    assert_eq!(hx.token.balance(&m0), 10_000 + 250);
    assert_eq!(hx.token.balance(&hx.contract_id), 0);

    // Auction is closed.
    assert_eq!(hx.client.get_sealed_auction().unwrap().open, false);
}

#[test]
#[should_panic(expected = "Revealed values do not match commitment")]
fn test_reveal_with_wrong_values_rejected() {
    let hx = setup(100);
    let m1 = hx.members.get(1).unwrap();
    let salt = BytesN::from_array(&hx.env, &[7u8; 32]);

    hx.client
        .commit_slot_bid(&m1, &commit_hash(&hx.env, &m1,300, &salt), &500);

    hx.env.ledger().set_timestamp(1_000 + COMMIT_DURATION + 1);
    // Reveal a different amount (400) than was committed (300) → hash mismatch.
    hx.client.reveal_slot_bid(&m1, &0, &400, &salt);
}

#[test]
#[should_panic(expected = "Reveal phase has not opened yet")]
fn test_reveal_before_reveal_phase_rejected() {
    // Sniping prevention: bids cannot be acted on while the commit phase is
    // still open.
    let hx = setup(100);
    let m1 = hx.members.get(1).unwrap();
    let salt = BytesN::from_array(&hx.env, &[7u8; 32]);

    hx.client
        .commit_slot_bid(&m1, &commit_hash(&hx.env, &m1,300, &salt), &300);

    // Still inside the commit phase — reveal must be rejected.
    hx.client.reveal_slot_bid(&m1, &0, &300, &salt);
}

#[test]
#[should_panic(expected = "Commit phase has closed")]
fn test_commit_after_commit_phase_rejected() {
    // Sniping prevention: no new bids once the commit phase closes.
    let hx = setup(100);
    let m1 = hx.members.get(1).unwrap();
    let salt = BytesN::from_array(&hx.env, &[7u8; 32]);

    hx.env.ledger().set_timestamp(1_000 + COMMIT_DURATION + 1);
    hx.client
        .commit_slot_bid(&m1, &commit_hash(&hx.env, &m1,300, &salt), &300);
}

#[test]
fn test_no_bid_above_reserve_leaves_slot_unallocated() {
    // Reserve is 1000; both bids are below it, so no winner is chosen and every
    // revealed deposit is refunded in full.
    let hx = setup(1_000);
    let m1 = hx.members.get(1).unwrap();
    let m2 = hx.members.get(2).unwrap();
    let salt1 = BytesN::from_array(&hx.env, &[7u8; 32]);
    let salt2 = BytesN::from_array(&hx.env, &[9u8; 32]);

    hx.client
        .commit_slot_bid(&m1, &commit_hash(&hx.env, &m1,300, &salt1), &300);
    hx.client
        .commit_slot_bid(&m2, &commit_hash(&hx.env, &m2,500, &salt2), &500);

    hx.env.ledger().set_timestamp(1_000 + COMMIT_DURATION + 1);
    hx.client.reveal_slot_bid(&m1, &0, &300, &salt1);
    hx.client.reveal_slot_bid(&m2, &0, &500, &salt2);

    hx.env
        .ledger()
        .set_timestamp(1_000 + COMMIT_DURATION + REVEAL_DURATION + 1);
    hx.client.settle_sealed_slot_auction();

    // No winner: every deposit refunded, nobody charged, nobody bonused.
    assert_eq!(hx.token.balance(&m1), 10_000);
    assert_eq!(hx.token.balance(&m2), 10_000);
    assert_eq!(hx.token.balance(&hx.contract_id), 0);
}

#[test]
fn test_unrevealed_commit_is_forfeited() {
    let hx = setup(100);
    let m1 = hx.members.get(1).unwrap();
    let m2 = hx.members.get(2).unwrap();
    let salt2 = BytesN::from_array(&hx.env, &[9u8; 32]);

    // m1 commits but never reveals; m2 commits and reveals.
    hx.client
        .commit_slot_bid(&m1, &commit_hash(&hx.env, &m1,300, &BytesN::from_array(&hx.env, &[7u8; 32])), &300);
    hx.client
        .commit_slot_bid(&m2, &commit_hash(&hx.env, &m2,500, &salt2), &500);

    hx.env.ledger().set_timestamp(1_000 + COMMIT_DURATION + 1);
    hx.client.reveal_slot_bid(&m2, &0, &500, &salt2);

    hx.env
        .ledger()
        .set_timestamp(1_000 + COMMIT_DURATION + REVEAL_DURATION + 1);
    hx.client.settle_sealed_slot_auction();

    // m1 never revealed → its 300 deposit is forfeited (not refunded). As a
    // non-winning member it still receives a share of the winning bid: m2 pays
    // 500, split 250/250 between m0 and m1.
    assert_eq!(hx.token.balance(&m1), 10_000 - 300 + 250);
    // m2 is the sole valid bidder and wins (500 > reserve 100), paying 500.
    assert_eq!(hx.token.balance(&m2), 10_000 - 500);
    // The forfeited 300 deposit remains held by the contract.
    assert_eq!(hx.token.balance(&hx.contract_id), 300);
}

// ── #392: deadline guards ─────────────────────────────────────────────────────

/// reveal_slot_bid called after reveal_until returns AuctionWindowClosed (103).
#[test]
fn test_late_reveal_rejected() {
    let hx = setup(100);
    let m1 = hx.members.get(1).unwrap();
    let salt = BytesN::from_array(&hx.env, &[7u8; 32]);

    hx.client.commit_slot_bid(&m1, &commit_hash(&hx.env, &m1, 300, &salt), &300);

    // Advance past the entire reveal window.
    hx.env.ledger().set_timestamp(1_000 + COMMIT_DURATION + REVEAL_DURATION + 1);

    let result = hx.client.try_reveal_slot_bid(&m1, &0, &300, &salt);
    assert!(result.is_err(), "Reveal after reveal_until must be rejected");
}

/// settle_sealed_slot_auction called before reveal_until returns AuctionWindowClosed (103).
#[test]
fn test_early_settle_rejected() {
    let hx = setup(100);
    let m1 = hx.members.get(1).unwrap();
    let salt = BytesN::from_array(&hx.env, &[7u8; 32]);

    hx.client.commit_slot_bid(&m1, &commit_hash(&hx.env, &m1, 300, &salt), &300);

    // Advance into the reveal phase but NOT past it.
    hx.env.ledger().set_timestamp(1_000 + COMMIT_DURATION + 1);
    hx.client.reveal_slot_bid(&m1, &0, &300, &salt);

    // Still inside the reveal window — settlement must be rejected.
    let result = hx.client.try_settle_sealed_slot_auction();
    assert!(result.is_err(), "Settle during reveal phase must be rejected");
}

/// A bid revealed within the window is accepted and appears in SealedRevealedBids.
#[test]
fn test_reveal_within_window_accepted() {
    let hx = setup(100);
    let m1 = hx.members.get(1).unwrap();
    let salt = BytesN::from_array(&hx.env, &[7u8; 32]);

    hx.client.commit_slot_bid(&m1, &commit_hash(&hx.env, &m1, 300, &salt), &300);

    // Advance to the start of the reveal phase.
    hx.env.ledger().set_timestamp(1_000 + COMMIT_DURATION + 1);
    hx.client.reveal_slot_bid(&m1, &0, &300, &salt);

    assert_eq!(hx.client.get_sealed_revealed_bids(&0).len(), 1);
}
