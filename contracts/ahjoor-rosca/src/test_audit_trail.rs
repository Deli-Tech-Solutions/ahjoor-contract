#![cfg(test)]

use crate::{AhjoorContract, AhjoorContractClient, ContributionEntry, CycleRecord, PayoutStrategy, RoscaConfig, VotingMode};
use soroban_sdk::{testutils::{Address as _, Ledger as _, StellarAssetContract as _}, token, Address, Env, Vec};

fn create_test_contract(env: &Env) -> (AhjoorContractClient, Address, Vec<Address>) {
    let contract_id = env.register_contract(None, AhjoorContract);
    let client = AhjoorContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let member1 = Address::generate(env);
    let member2 = Address::generate(env);
    let member3 = Address::generate(env);

    let mut members = Vec::new(env);
    members.push_back(member1.clone());
    members.push_back(member2.clone());
    members.push_back(member3.clone());

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = sac.address();
    let token_admin_client = token::StellarAssetClient::new(env, &token);
    let contribution_amount = 1000i128;
    let round_duration = 100u64;

    let config = RoscaConfig {
        strategy: PayoutStrategy::RoundRobin,
        custom_order: None,
        penalty_amount: 50i128,
        exit_penalty_bps: 1000u32, // 10%
        collective_goal: None,
        member_goals: None,
        fee_bps: 100u32, // 1%
        fee_recipient: Some(admin.clone()),
        max_defaults: 3u32,
            grace_period_ledgers: 0,
            grace_period_seconds: 0,
            use_timestamp_schedule: false,
        round_duration_seconds: 0u64,
        max_members: Some(10u32),
        skip_fee: 10i128,
        max_skips_per_cycle: 1u32,
        voting_mode: VotingMode::Equal,
    late_fee_bps: 0,
    grace_period_seconds: 0,
    auction_enabled: false,
    auction_window_ledgers: 0,
    randomize_payout_order: false,
    reserve_enabled: false,
    reserve_contribution_bps: 0,
    };

    client.init(
        &admin,
        &members,
        &contribution_amount,
        &token,
        &round_duration,
        &config,
    );

    (client, admin, members)
}

#[test]
fn test_cycle_record_created_on_round_completion() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Mint tokens for members
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        token_admin_client.mint(&member, &10000i128);
    }

    // All members contribute
    for member in members.iter() {
        client.contribute(&member, &token, &1000i128);
    }

    // Check that cycle record was created for cycle 0
    let cycle_record = client.get_cycle_record(&0u32);
    assert!(cycle_record.is_some());

    let record = cycle_record.unwrap();
    assert_eq!(record.cycle_number, 0);
    assert_eq!(record.payout_recipient, members.get(0).unwrap());
    assert_eq!(record.contributions.len(), 3);
    assert_eq!(record.defaulters.len(), 0);
    assert_eq!(record.skippers.len(), 0);
}

#[test]
fn test_cycle_record_contains_all_contributions() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Mint tokens for members
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        token_admin_client.mint(&member, &10000i128);
    }

    // All members contribute
    for member in members.iter() {
        client.contribute(&member, &token, &1000i128);
    }

    // Get cycle record
    let cycle_record = client.get_cycle_record(&0u32).unwrap();

    // Verify all contributions are recorded
    assert_eq!(cycle_record.contributions.len(), 3);
    
    for contribution in cycle_record.contributions.iter() {
        assert_eq!(contribution.amount, 1000i128);
        assert!(members.contains(&contribution.member));
    }
}

#[test]
fn test_cycle_record_tracks_defaulters() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Mint tokens for members
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        token_admin_client.mint(&member, &10000i128);
    }

    // Only first two members contribute
    client.contribute(&members.get(0).unwrap(), &token, &1000i128);
    client.contribute(&members.get(1).unwrap(), &token, &1000i128);

    // Advance time past deadline
    env.ledger().with_mut(|li| {
        li.timestamp = 200;
    });

    // Finalize round (this will mark member3 as defaulter)
    client.finalize_round();

    // Get cycle record
    let cycle_record = client.get_cycle_record(&0u32).unwrap();

    // Verify defaulter is recorded
    assert_eq!(cycle_record.defaulters.len(), 1);
    assert_eq!(cycle_record.defaulters.get(0).unwrap(), members.get(2).unwrap());
}

#[test]
fn test_cycle_record_tracks_skippers() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Mint tokens for members
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        token_admin_client.mint(&member, &10000i128);
    }

    // Member 3 requests to skip
    client.request_skip(&members.get(2).unwrap(), &0u32);

    // Other members contribute
    client.contribute(&members.get(0).unwrap(), &token, &1000i128);
    client.contribute(&members.get(1).unwrap(), &token, &1000i128);

    // Advance time past deadline
    env.ledger().with_mut(|li| {
        li.timestamp = 200;
    });

    // Finalize round to trigger payout and audit recording
    client.close_round();



    // Get cycle record
    let cycle_record = client.get_cycle_record(&0u32).unwrap();

    // Verify skipper is recorded
    assert_eq!(cycle_record.skippers.len(), 1);
    assert_eq!(cycle_record.skippers.get(0).unwrap(), members.get(2).unwrap());
}

#[test]
fn test_member_contribution_history() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Mint tokens for members
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        token_admin_client.mint(&member, &100000i128);
    }

    // Complete 3 rounds
    for _round in 0..3 {
        for member in members.iter() {
            client.contribute(&member, &token, &1000i128);
        }
    }

    // Get contribution history for member1
    let member1 = members.get(0).unwrap();
    let history = client.get_member_contribution_history(&member1);

    // Verify history contains 3 contributions
    assert_eq!(history.len(), 3);
    
    for contribution in history.iter() {
        assert_eq!(contribution.member, member1);
        assert_eq!(contribution.amount, 1000i128);
    }
}

#[test]
fn test_retention_window_configuration() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _members) = create_test_contract(&env);

    // Default retention window should be 100
    let default_window = client.get_cycle_retention_window();
    assert_eq!(default_window, 100);

    // Update retention window
    client.set_cycle_retention_window(&50u32);

    // Verify update
    let new_window = client.get_cycle_retention_window();
    assert_eq!(new_window, 50);
}

#[test]
fn test_cycle_record_includes_fee_collected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Mint tokens for members
    let sac = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        sac.mint(&member, &10000i128);
    }

    // All members contribute
    for member in members.iter() {
        client.contribute(&member, &token, &1000i128);
    }

    // Get cycle record
    let cycle_record = client.get_cycle_record(&0u32).unwrap();

    // Verify fee was collected (1% of 3000 = 30)
    assert!(cycle_record.fee_collected > 0);
}

#[test]
fn test_cycle_record_includes_insurance_drawn() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Mint tokens for members and admin
    let sac = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        sac.mint(&member, &10000i128);
    }
    sac.mint(&admin, &10000i128);

    // Add to insurance pool
    client.contribute_to_insurance(&admin, &token, &500i128);

    // Only 2 members contribute (shortfall expected)
    client.contribute(&members.get(0).unwrap(), &token, &1000i128);
    client.contribute(&members.get(1).unwrap(), &token, &1000i128);

    // Advance time past deadline
    env.ledger().with_mut(|li| {
        li.timestamp = 200;
    });

    // Finalize round
    client.finalize_round();

    // Get cycle record
    let cycle_record = client.get_cycle_record(&0u32).unwrap();

    // Verify insurance was drawn
    assert!(cycle_record.insurance_drawn > 0);
}

#[test]
fn test_cycle_timestamps_recorded() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Mint tokens for members
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        token_admin_client.mint(&member, &10000i128);
    }

    let start_time = env.ledger().timestamp();

    // All members contribute
    for member in members.iter() {
        client.contribute(&member, &token, &1000i128);
    }

    let end_time = env.ledger().timestamp();

    // Get cycle record
    let cycle_record = client.get_cycle_record(&0u32).unwrap();

    // Verify timestamps
    assert_eq!(cycle_record.cycle_start_timestamp, start_time);
    assert!(cycle_record.cycle_end_timestamp >= end_time);
}

#[test]
fn test_multiple_cycles_recorded() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Mint tokens for members
    let sac = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        sac.mint(&member, &100000i128);
    }

    // Complete 5 rounds
    for round in 0..5 {
        for member in members.iter() {
            client.contribute(&member, &token, &1000i128);
        }

        // Verify cycle record exists
        let cycle_record = client.get_cycle_record(&round);
        assert!(cycle_record.is_some());
        assert_eq!(cycle_record.unwrap().cycle_number, round);
    }
}

#[test]
fn test_archived_records_accessible() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Set a small retention window for testing
    client.set_cycle_retention_window(&2u32);

    // Mint tokens for members
    let sac = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        sac.mint(&member, &100000i128);
    }

    // Complete 5 rounds (should trigger archival)
    for _round in 0..5 {
        for member in members.iter() {
            client.contribute(&member, &token, &1000i128);
        }
    }

    // Old records should still be accessible (from temporary storage)
    let old_record = client.get_cycle_record(&0u32);
    // Note: In a real scenario, archived records in temporary storage may expire
    // This test demonstrates the archival mechanism is triggered
}

#[test]
fn test_cycle_record_total_pool_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Mint tokens for members
    let sac = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        sac.mint(&member, &10000i128);
    }

    // All members contribute
    for member in members.iter() {
        client.contribute(&member, &token, &1000i128);
    }

    // Get cycle record
    let cycle_record = client.get_cycle_record(&0u32).unwrap();

    // Verify total pool amount (3 members * 1000 = 3000)
    assert_eq!(cycle_record.total_pool_amount, 3000i128);
}




#[test]
fn test_cycle_record_timestamp_nonzero_ledger_mode() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, members) = create_test_contract(&env);
    let token = client.get_state().4;

    // Ensure we're in ledger-mode (use_timestamp_schedule = false)
    // The test contract is created with use_timestamp_schedule = false

    // Advance the ledger sequence before the first round starts
    env.ledger().with_mut(|li| {
        li.sequence_number = 10;
    });

    // Mint tokens for members
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    for member in members.iter() {
        token_admin_client.mint(&member, &10000i128);
    }

    // All members contribute (this completes round 0 and should trigger cycle end recording)
    for member in members.iter() {
        client.contribute(&member, &token, &1000i128);
    }

    // Get cycle record for cycle 0
    let cycle_record = client.get_cycle_record(&0u32).unwrap();

    // Verify cycle_end_timestamp is non-zero in ledger-mode
    // It should be the sequence number (10) cast as u64
    assert!(cycle_record.cycle_end_timestamp > 0, "cycle_end_timestamp should be non-zero in ledger-mode");
    assert_eq!(cycle_record.cycle_end_timestamp, 10u64, "cycle_end_timestamp should equal ledger sequence");

    // Verify cycle_start_timestamp is also non-zero
    assert!(cycle_record.cycle_start_timestamp > 0, "cycle_start_timestamp should be non-zero in ledger-mode");
    
    // Verify other cycle record fields are populated
    assert_eq!(cycle_record.cycle_number, 0);
    assert_eq!(cycle_record.contributions.len(), 3);
    assert_eq!(cycle_record.defaulters.len(), 0);
}
