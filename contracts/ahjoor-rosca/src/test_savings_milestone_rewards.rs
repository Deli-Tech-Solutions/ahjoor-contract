#![cfg(test)]
use super::*;
use crate::savings_goal_tracking::{GoalStatus, Milestone, RewardType};
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::Address as _, Address, Env, Map, String, Vec};

fn setup_rosca<'a>() -> (
    Env,
    AhjoorContractClient<'a>,
    Address,
    Address,
    Address,
    TokenAdminClient<'a>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let member = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let tac = TokenAdminClient::new(&env, &token_addr);

    tac.mint(&admin, &1_000_000);
    tac.mint(&member, &1_000_000);

    let mut members = Vec::new(&env);
    members.push_back(member.clone());

    client.init(
        &admin,
        &members,
        &100i128,
        &token_addr,
        &3600u64,
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

    (env, client, admin, member, token_addr, tac)
}

fn make_milestone(env: &Env, id: u32, pct: u32, reward_bps: u32) -> Milestone {
    Milestone {
        milestone_id: id,
        percentage: pct,
        amount: 1,
        name: String::from_str(env, "test"),
        description: String::from_str(env, ""),
        reward_type: RewardType::Bonus,
        reward_value: 0,
        celebration_event: String::from_str(env, ""),
        reward_bps,
    }
}

#[test]
fn test_milestone_reward_distributed_once() {
    let (env, client, admin, member, token_addr, tac) = setup_rosca();

    // Fund the reward pool
    client.fund_savings_reward_pool(&admin, &10_000i128);
    assert_eq!(client.get_savings_reward_pool(), 10_000i128);

    // Create a savings goal
    let goal_id = client.create_savings_goal(
        &member,
        &1u32,
        &String::from_str(&env, "vacation"),
        &String::from_str(&env, ""),
        &1_000i128,
        &token_addr,
        &(env.ledger().timestamp() + 86400),
        &1u32,
        &String::from_str(&env, "travel"),
        &Map::new(&env),
    );

    // Add a 25% milestone with 1000 bps reward (10% of contribution)
    let mut milestones = Vec::new(&env);
    milestones.push_back(make_milestone(&env, 1, 25, 1_000));
    client.add_savings_goal_milestones(&goal_id, &milestones);

    let member_balance_before = soroban_sdk::token::Client::new(&env, &token_addr).balance(&member);

    // Contribute 250 (25% of 1000) — should cross the milestone
    client.contribute_to_savings_goal(
        &goal_id,
        &member,
        &250i128,
        &String::from_str(&env, "manual"),
    );

    let member_balance_after = soroban_sdk::token::Client::new(&env, &token_addr).balance(&member);

    // contribute_to_savings_goal tracks the amount but doesn't debit the wallet;
    // only the reward transfer (pool → member) changes the balance.
    // Reward = 250 * 1000 / 10000 = 25
    assert_eq!(member_balance_after - member_balance_before, 25);

    // Pool reduced by 25
    assert_eq!(client.get_savings_reward_pool(), 10_000 - 25);

    // Bitmask bit 1 should be set
    let bitmask = client.get_savings_milestones_claimed(&goal_id, &member);
    assert_eq!(bitmask & 2, 2); // bit for milestone_id=1
}

#[test]
fn test_milestone_reward_distributed_exactly_once() {
    let (env, client, admin, member, token_addr, _tac) = setup_rosca();

    client.fund_savings_reward_pool(&admin, &10_000i128);

    let goal_id = client.create_savings_goal(
        &member, &1u32,
        &String::from_str(&env, "goal"),
        &String::from_str(&env, ""),
        &1_000i128, &token_addr,
        &(env.ledger().timestamp() + 86400),
        &1u32, &String::from_str(&env, "test"),
        &Map::new(&env),
    );

    let mut milestones = Vec::new(&env);
    milestones.push_back(make_milestone(&env, 1, 25, 1_000));
    client.add_savings_goal_milestones(&goal_id, &milestones);

    // First contribution crosses the milestone
    client.contribute_to_savings_goal(&goal_id, &member, &250i128, &String::from_str(&env, "m"));
    let pool_after_first = client.get_savings_reward_pool();

    // Second contribution — milestone already claimed, no reward
    client.contribute_to_savings_goal(&goal_id, &member, &100i128, &String::from_str(&env, "m"));
    let pool_after_second = client.get_savings_reward_pool();

    assert_eq!(pool_after_first, pool_after_second); // No additional reward
}

#[test]
fn test_reward_pool_depletion_does_not_revert() {
    let (env, client, admin, member, token_addr, _tac) = setup_rosca();

    // Fund pool with only 1 token (less than reward would be)
    client.fund_savings_reward_pool(&admin, &1i128);

    let goal_id = client.create_savings_goal(
        &member, &1u32,
        &String::from_str(&env, "goal"),
        &String::from_str(&env, ""),
        &1_000i128, &token_addr,
        &(env.ledger().timestamp() + 86400),
        &1u32, &String::from_str(&env, "test"),
        &Map::new(&env),
    );

    // 1000 bps of 250 = 25, but pool only has 1 — gracefully skips
    let mut milestones = Vec::new(&env);
    milestones.push_back(make_milestone(&env, 1, 25, 1_000));
    client.add_savings_goal_milestones(&goal_id, &milestones);

    // Should not revert even though pool < reward
    client.contribute_to_savings_goal(&goal_id, &member, &250i128, &String::from_str(&env, "m"));
    // milestone is still claimed in bitmask
    let bitmask = client.get_savings_milestones_claimed(&goal_id, &member);
    assert_eq!(bitmask & 2, 2);
}

#[test]
fn test_multiple_milestones_crossed_in_one_contribution() {
    let (env, client, admin, member, token_addr, _tac) = setup_rosca();

    client.fund_savings_reward_pool(&admin, &100_000i128);

    let goal_id = client.create_savings_goal(
        &member, &1u32,
        &String::from_str(&env, "goal"),
        &String::from_str(&env, ""),
        &1_000i128, &token_addr,
        &(env.ledger().timestamp() + 86400),
        &1u32, &String::from_str(&env, "test"),
        &Map::new(&env),
    );

    let mut milestones = Vec::new(&env);
    milestones.push_back(make_milestone(&env, 1, 25, 500));  // 5% reward
    milestones.push_back(make_milestone(&env, 2, 50, 500));  // 5% reward
    client.add_savings_goal_milestones(&goal_id, &milestones);

    // Single contribution of 600 crosses both 25% and 50% thresholds
    let pool_before = client.get_savings_reward_pool();
    client.contribute_to_savings_goal(&goal_id, &member, &600i128, &String::from_str(&env, "m"));
    let pool_after = client.get_savings_reward_pool();

    // Both milestones trigger: each gives 600 * 500 / 10000 = 30
    assert_eq!(pool_before - pool_after, 60);

    let bitmask = client.get_savings_milestones_claimed(&goal_id, &member);
    assert_eq!(bitmask & 2, 2); // milestone_id=1
    assert_eq!(bitmask & 4, 4); // milestone_id=2
}

#[test]
fn test_admin_can_fund_reward_pool_post_init() {
    let (env, client, admin, _member, _token_addr, _tac) = setup_rosca();

    assert_eq!(client.get_savings_reward_pool(), 0);

    client.fund_savings_reward_pool(&admin, &5_000i128);
    assert_eq!(client.get_savings_reward_pool(), 5_000i128);

    client.fund_savings_reward_pool(&admin, &3_000i128);
    assert_eq!(client.get_savings_reward_pool(), 8_000i128);
}
