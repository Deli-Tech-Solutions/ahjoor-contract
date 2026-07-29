#![cfg(test)]
use crate::{ProposalStatus, TokenWhitelistContract, TokenWhitelistContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;

fn setup_governance<'a>() -> (
    Env,
    Address,                          // admin
    TokenWhitelistContractClient<'a>, // whitelist client
    Address,                          // governance token address
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Deploy a stellar asset as the governance token
    let gov_token = env.register_stellar_asset_contract_v2(admin.clone()).address();

    // Configure governance
    client.set_governance_token(&admin, &gov_token);
    client.set_min_proposal_stake(&admin, &100i128);
    client.set_voting_window_ledgers(&admin, &100u32);
    client.set_enactment_delay_ledgers(&admin, &50u32);
    client.set_quorum_bps(&admin, &5_000u32); // 50%

    (env, admin, client, gov_token)
}

fn mint_gov_tokens(env: &Env, gov_token: &Address, admin: &Address, to: &Address, amount: i128) {
    TokenAdminClient::new(env, gov_token).mint(to, &amount);
}

#[test]
fn test_full_proposal_vote_enact_lifecycle() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter1, 600);
    mint_gov_tokens(&env, &gov_token, &admin, &voter2, 400);

    let rationale = BytesN::from_array(&env, &[1u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // Vote: 600 approve, 400 reject → 60% approve → quorum met (>= 50%)
    client.vote_listing(&voter1, &proposal_id, &true, &600i128);
    client.vote_listing(&voter2, &proposal_id, &false, &400i128);

    // Close voting window
    env.ledger().set_sequence_number(env.ledger().sequence() + 101);

    client.finalise_listing_proposal(&proposal_id);

    let proposal = client.get_listing_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::PendingEnactment);

    // Advance past enactment delay
    env.ledger().set_sequence_number(env.ledger().sequence() + 51);

    client.enact_listing(&proposal_id);

    let proposal = client.get_listing_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Enacted);

    // Token now in whitelist
    assert!(client.is_token_allowed(&new_token));
}

#[test]
fn test_quorum_failure_marks_proposal_failed() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 400);

    let rationale = BytesN::from_array(&env, &[2u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // Only 40% approve → below 50% quorum
    client.vote_listing(&voter, &proposal_id, &true, &400i128);
    // No reject votes, total = 400, approve = 400 but we need another voter to reject
    // Actually 400/400 = 100% approve. Let's add a reject voter to bring below threshold
    let rejecter = Address::generate(&env);
    mint_gov_tokens(&env, &gov_token, &admin, &rejecter, 700);
    client.vote_listing(&rejecter, &proposal_id, &false, &700i128);
    // Now approve=400, total=1100, approve% = 36% < 50%

    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_listing_proposal(&proposal_id);

    let proposal = client.get_listing_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Failed);
}

#[test]
fn test_admin_veto_blocks_enactment() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 600);

    let rationale = BytesN::from_array(&env, &[3u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    client.vote_listing(&voter, &proposal_id, &true, &600i128);
    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_listing_proposal(&proposal_id);

    // Admin vetoes before enactment
    let reason = BytesN::from_array(&env, &[0xFFu8; 32]);
    client.veto_listing_proposal(&admin, &proposal_id, &reason);

    let proposal = client.get_listing_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Vetoed);

    // Token NOT in whitelist
    assert!(!client.is_token_allowed(&new_token));
}

#[test]
#[should_panic(expected = "InsufficientProposerStake")]
fn test_insufficient_proposer_stake_rejected() {
    let (env, _admin, client, gov_token) = setup_governance();

    let poor_proposer = Address::generate(&env);
    // Only mint 50, but minimum is 100
    mint_gov_tokens(&env, &gov_token, &_admin, &poor_proposer, 50);

    let new_token = Address::generate(&env);
    let rationale = BytesN::from_array(&env, &[4u8; 32]);
    client.propose_token_listing(&poor_proposer, &new_token, &rationale);
}

#[test]
fn test_duplicate_vote_overwrite_last_write_wins() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 800);

    let rationale = BytesN::from_array(&env, &[5u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // First vote: approve 800
    client.vote_listing(&voter, &proposal_id, &true, &800i128);

    let p = client.get_listing_proposal(&proposal_id);
    assert_eq!(p.approve_weight, 800);

    // Overwrite: now reject 800
    client.vote_listing(&voter, &proposal_id, &false, &800i128);

    let p = client.get_listing_proposal(&proposal_id);
    assert_eq!(p.approve_weight, 0);
    assert_eq!(p.reject_weight, 800);
}

#[test]
#[should_panic(expected = "VotingWindowClosed")]
fn test_cannot_vote_after_window_closes() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 500);

    let rationale = BytesN::from_array(&env, &[6u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // Close voting window
    env.ledger().set_sequence_number(env.ledger().sequence() + 101);

    client.vote_listing(&voter, &proposal_id, &true, &500i128);
}

#[test]
#[should_panic(expected = "VoteWeightExceedsBalance")]
fn test_vote_weight_exceeds_balance_rejected() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 300);

    let rationale = BytesN::from_array(&env, &[7u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // Try to vote with weight > balance
    client.vote_listing(&voter, &proposal_id, &true, &1000i128);
}

#[test]
#[should_panic(expected = "EnactmentDelayNotElapsed")]
fn test_cannot_enact_before_delay() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 600);

    let rationale = BytesN::from_array(&env, &[8u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);
    client.vote_listing(&voter, &proposal_id, &true, &600i128);

    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_listing_proposal(&proposal_id);

    // Enactment delay not elapsed yet
    client.enact_listing(&proposal_id);
}

// ─── Governance-gated delisting (mirrors the listing tests above; #3) ────────

#[test]
fn test_full_delisting_proposal_vote_enact_lifecycle() {
    let (env, admin, client, gov_token) = setup_governance();

    let listed_token = Address::generate(&env);
    client.add_token(&admin, &listed_token);
    assert!(client.is_token_allowed(&listed_token));

    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter1, 600);
    mint_gov_tokens(&env, &gov_token, &admin, &voter2, 400);

    let rationale = BytesN::from_array(&env, &[10u8; 32]);
    let proposal_id = client.propose_delisting(&proposer, &listed_token, &rationale);

    // Vote: 600 approve, 400 reject → 60% approve → quorum met (>= 50%)
    client.vote_delisting(&voter1, &proposal_id, &true, &600i128);
    client.vote_delisting(&voter2, &proposal_id, &false, &400i128);

    // Close voting window
    env.ledger().set_sequence_number(env.ledger().sequence() + 101);

    client.finalise_delisting_proposal(&proposal_id);

    let proposal = client.get_delisting_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::PendingEnactment);

    // Advance past enactment delay
    env.ledger().set_sequence_number(env.ledger().sequence() + 51);

    client.enact_delisting(&proposal_id);

    let proposal = client.get_delisting_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Enacted);

    // Token no longer in whitelist
    assert!(!client.is_token_allowed(&listed_token));
}

#[test]
fn test_delisting_quorum_failure_marks_proposal_failed() {
    let (env, admin, client, gov_token) = setup_governance();

    let listed_token = Address::generate(&env);
    client.add_token(&admin, &listed_token);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let rejecter = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 400);
    mint_gov_tokens(&env, &gov_token, &admin, &rejecter, 700);

    let rationale = BytesN::from_array(&env, &[11u8; 32]);
    let proposal_id = client.propose_delisting(&proposer, &listed_token, &rationale);

    client.vote_delisting(&voter, &proposal_id, &true, &400i128);
    client.vote_delisting(&rejecter, &proposal_id, &false, &700i128);
    // approve=400, total=1100, approve% = 36% < 50%

    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_delisting_proposal(&proposal_id);

    let proposal = client.get_delisting_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Failed);

    // Token remains whitelisted since delisting failed
    assert!(client.is_token_allowed(&listed_token));
}

#[test]
fn test_admin_veto_blocks_delisting_enactment() {
    let (env, admin, client, gov_token) = setup_governance();

    let listed_token = Address::generate(&env);
    client.add_token(&admin, &listed_token);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 600);

    let rationale = BytesN::from_array(&env, &[12u8; 32]);
    let proposal_id = client.propose_delisting(&proposer, &listed_token, &rationale);

    client.vote_delisting(&voter, &proposal_id, &true, &600i128);
    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_delisting_proposal(&proposal_id);

    // Admin vetoes before enactment
    let reason = BytesN::from_array(&env, &[0xEEu8; 32]);
    client.veto_delisting_proposal(&admin, &proposal_id, &reason);

    let proposal = client.get_delisting_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Vetoed);

    // Token remains whitelisted; veto blocked the removal
    assert!(client.is_token_allowed(&listed_token));
}

#[test]
#[should_panic(expected = "TokenNotWhitelisted")]
fn test_cannot_propose_delisting_for_non_whitelisted_token() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);

    let never_listed = Address::generate(&env);
    let rationale = BytesN::from_array(&env, &[13u8; 32]);
    client.propose_delisting(&proposer, &never_listed, &rationale);
}

#[test]
#[should_panic(expected = "InsufficientProposerStake")]
fn test_delisting_insufficient_proposer_stake_rejected() {
    let (env, admin, client, gov_token) = setup_governance();

    let listed_token = Address::generate(&env);
    client.add_token(&admin, &listed_token);

    let poor_proposer = Address::generate(&env);
    mint_gov_tokens(&env, &gov_token, &admin, &poor_proposer, 50);

    let rationale = BytesN::from_array(&env, &[14u8; 32]);
    client.propose_delisting(&poor_proposer, &listed_token, &rationale);
}

#[test]
#[should_panic(expected = "VotingWindowClosed")]
fn test_cannot_vote_delisting_after_window_closes() {
    let (env, admin, client, gov_token) = setup_governance();

    let listed_token = Address::generate(&env);
    client.add_token(&admin, &listed_token);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 500);

    let rationale = BytesN::from_array(&env, &[15u8; 32]);
    let proposal_id = client.propose_delisting(&proposer, &listed_token, &rationale);

    env.ledger().set_sequence_number(env.ledger().sequence() + 101);

    client.vote_delisting(&voter, &proposal_id, &true, &500i128);
}

#[test]
#[should_panic(expected = "EnactmentDelayNotElapsed")]
fn test_cannot_enact_delisting_before_delay() {
    let (env, admin, client, gov_token) = setup_governance();

    let listed_token = Address::generate(&env);
    client.add_token(&admin, &listed_token);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 600);

    let rationale = BytesN::from_array(&env, &[16u8; 32]);
    let proposal_id = client.propose_delisting(&proposer, &listed_token, &rationale);
    client.vote_delisting(&voter, &proposal_id, &true, &600i128);

    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_delisting_proposal(&proposal_id);

    // Enactment delay not elapsed yet
    client.enact_delisting(&proposal_id);
}

#[test]
fn test_delisting_and_listing_proposal_ids_are_independent() {
    let (env, admin, client, gov_token) = setup_governance();

    let listed_token = Address::generate(&env);
    client.add_token(&admin, &listed_token);
    let new_token = Address::generate(&env);

    let proposer = Address::generate(&env);
    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);

    let listing_rationale = BytesN::from_array(&env, &[17u8; 32]);
    let listing_id = client.propose_token_listing(&proposer, &new_token, &listing_rationale);

    let delisting_rationale = BytesN::from_array(&env, &[18u8; 32]);
    let delisting_id = client.propose_delisting(&proposer, &listed_token, &delisting_rationale);

    // Both counters start independently at 0
    assert_eq!(listing_id, 0);
    assert_eq!(delisting_id, 0);

    // Each proposal resolves to the correct token via its own namespace
    assert_eq!(client.get_listing_proposal(&listing_id).token, new_token);
    assert_eq!(client.get_delisting_proposal(&delisting_id).token, listed_token);
}

#[test]
fn test_enact_delisting_clears_active_suspension() {
    let (env, admin, client, gov_token) = setup_governance();

    let listed_token = Address::generate(&env);
    client.add_token(&admin, &listed_token);
    client.suspend_token_timed(
        &admin,
        &listed_token,
        &1000u32,
        &BytesN::from_array(&env, &[19u8; 32]),
    );
    assert!(client.get_token_suspension(&listed_token).is_some());

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 600);

    let rationale = BytesN::from_array(&env, &[20u8; 32]);
    let proposal_id = client.propose_delisting(&proposer, &listed_token, &rationale);
    client.vote_delisting(&voter, &proposal_id, &true, &600i128);

    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_delisting_proposal(&proposal_id);
    env.ledger().set_sequence_number(env.ledger().sequence() + 51);
    client.enact_delisting(&proposal_id);

    // Suspension record was cleared along with the whitelist entry
    assert!(client.get_token_suspension(&listed_token).is_none());
}
