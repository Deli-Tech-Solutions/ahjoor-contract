#![allow(dead_code)]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Bytes,
    BytesN, Env, Map, String, Symbol, Vec,
};
use crate::savings_goal_tracking::*;
use crate::types::{DataKey, DataKey3};

// Storage keys
const GOAL_COUNTER_KEY: &str = "goal_counter";
const GOAL_KEY_PREFIX: &str = "goal_";
const CONTRIBUTION_COUNTER_KEY: &str = "contribution_counter";
const CONTRIBUTION_KEY_PREFIX: &str = "contribution_";
const CELEBRATION_COUNTER_KEY: &str = "celebration_counter";
const CELEBRATION_KEY_PREFIX: &str = "celebration_";
const BADGE_COUNTER_KEY: &str = "badge_counter";
const BADGE_KEY_PREFIX: &str = "badge_";
const MEMBER_GOALS_KEY_PREFIX: &str = "member_goals_";
const GROUP_GOALS_KEY_PREFIX: &str = "group_goals_";

/// Implementation of savings goal tracking functionality
pub struct SavingsGoalTrackingImpl;

impl SavingsGoalTrackingImpl {
    /// Create a new savings goal
    pub fn create_goal(
        env: &Env,
        member: Address,
        group_id: u32,
        name: String,
        description: String,
        target_amount: i128,
        token: Address,
        target_date: u64,
        priority: u32,
        category: String,
        metadata: Map<String, String>,
    ) -> u32 {
        member.require_auth();

        if target_amount <= 0 {
            panic_with_error!(env, SavingsGoalError::InvalidGoalAmount);
        }

        let now = env.ledger().timestamp();

        if target_date <= now {
            panic_with_error!(env, SavingsGoalError::GoalExpired);
        }

        // Get next goal ID
        let goal_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, GOAL_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_id = goal_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, SavingsGoalError::InvalidGoalAmount);
        });

        let goal = SavingsGoal {
            goal_id: next_id,
            member: member.clone(),
            group_id,
            name,
            description,
            target_amount,
            current_amount: 0,
            token,
            created_at: now,
            target_date,
            status: GoalStatus::Active,
            priority,
            category,
            milestones: Vec::new(env),
            completed_milestones: Vec::new(env),
            metadata,
        };

        // Store goal
        let key = (Symbol::new(env, GOAL_KEY_PREFIX), next_id);
        env.storage().persistent().set(&key, &goal);
        env.storage()
            .instance()
            .set(&Symbol::new(env, GOAL_COUNTER_KEY), &next_id);

        // Add to member goals list
        let member_key = (Symbol::new(env, MEMBER_GOALS_KEY_PREFIX), member.clone());
        let mut member_goals: Vec<u32> = env
            .storage()
            .persistent()
            .get(&member_key)
            .unwrap_or_else(|| Vec::new(env));
        member_goals.push_back(next_id);
        env.storage().persistent().set(&member_key, &member_goals);

        // Add to group goals list
        let group_key = (Symbol::new(env, GROUP_GOALS_KEY_PREFIX), group_id);
        let mut group_goals: Vec<u32> = env
            .storage()
            .persistent()
            .get(&group_key)
            .unwrap_or_else(|| Vec::new(env));
        group_goals.push_back(next_id);
        env.storage().persistent().set(&group_key, &group_goals);

        next_id
    }

    /// Add milestones to a goal
    pub fn add_milestones(env: &Env, goal_id: u32, milestones: Vec<Milestone>) {
        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::GoalNotFound));

        goal.member.require_auth();

        for milestone in milestones.iter() {
            if milestone.percentage == 0 || milestone.percentage > 100 {
                panic_with_error!(env, SavingsGoalError::InvalidMilestone);
            }
            if milestone.amount <= 0 {
                panic_with_error!(env, SavingsGoalError::InvalidMilestone);
            }
        }

        for milestone in milestones.iter() {
            goal.milestones.push_back(milestone);
        }

        env.storage().persistent().set(&key, &goal);
    }

    /// Contribute to a goal
    pub fn contribute_to_goal(
        env: &Env,
        goal_id: u32,
        member: Address,
        amount: i128,
        source: String,
    ) -> GoalContribution {
        member.require_auth();

        if amount <= 0 {
            panic_with_error!(env, SavingsGoalError::InvalidContribution);
        }

        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::GoalNotFound));

        // Check goal status
        match goal.status {
            GoalStatus::Completed => {
                panic_with_error!(env, SavingsGoalError::GoalCompleted);
            }
            GoalStatus::Abandoned => {
                panic_with_error!(env, SavingsGoalError::GoalAbandoned);
            }
            GoalStatus::Failed => {
                panic_with_error!(env, SavingsGoalError::GoalExpired);
            }
            _ => {}
        }

        let now = env.ledger().timestamp();

        // Check if goal has expired
        if now > goal.target_date {
            goal.status = GoalStatus::Failed;
            env.storage().persistent().set(&key, &goal);
            panic_with_error!(env, SavingsGoalError::GoalExpired);
        }

        // Update goal amount
        let new_amount = goal
            .current_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::InvalidContribution));

        if new_amount > goal.target_amount {
            panic_with_error!(env, SavingsGoalError::InvalidContribution);
        }

        goal.current_amount = new_amount;

        // Check if goal is completed
        if new_amount >= goal.target_amount {
            goal.status = GoalStatus::Completed;
        }

        // Get contribution ID
        let contribution_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, CONTRIBUTION_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_contribution_id = contribution_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, SavingsGoalError::InvalidContribution);
        });

        let contribution = GoalContribution {
            contribution_id: next_contribution_id,
            goal_id,
            member,
            amount,
            timestamp: now,
            source,
            tx_hash: BytesN::from_array(env, &[0u8; 32]),
        };

        // Store contribution
        let contribution_key = (Symbol::new(env, CONTRIBUTION_KEY_PREFIX), next_contribution_id);
        env.storage().persistent().set(&contribution_key, &contribution);

        // Store updated goal
        env.storage().persistent().set(&key, &goal);
        env.storage()
            .instance()
            .set(&Symbol::new(env, CONTRIBUTION_COUNTER_KEY), &next_contribution_id);

        // #359: Check milestones with reward_bps > 0 and distribute rewards
        Self::check_and_distribute_milestone_rewards(env, goal_id, &goal, amount);

        contribution
    }

    /// #359: Check whether any milestones have been newly crossed and transfer rewards.
    /// Rewards are transferred from the SavingsRewardPool. Pool depletion skips reward transfer
    /// without reverting.
    pub fn check_and_distribute_milestone_rewards(
        env: &Env,
        goal_id: u32,
        goal: &SavingsGoal,
        contribution_amount: i128,
    ) {
        if goal.target_amount == 0 { return; }
        let pct_now = ((goal.current_amount * 100) / goal.target_amount) as u32;

        for milestone in goal.milestones.iter() {
            if milestone.reward_bps == 0 { continue; }

            // Skip milestones already claimed (bitmask check)
            let bitmask_key = DataKey3::SavingsMilestonesClaimed(goal_id, goal.member.clone());
            let bitmask: u64 = env
                .storage()
                .persistent()
                .get(&bitmask_key)
                .unwrap_or(0u64);

            let bit = 1u64 << (milestone.milestone_id as u64 % 64);
            if bitmask & bit != 0 { continue; }

            // Check if threshold newly crossed
            let pct_before = if contribution_amount >= goal.current_amount {
                0u32
            } else {
                (((goal.current_amount - contribution_amount) * 100) / goal.target_amount) as u32
            };

            if pct_before < milestone.percentage && pct_now >= milestone.percentage {
                let reward_amount = (contribution_amount * milestone.reward_bps as i128) / 10_000;
                if reward_amount > 0 {
                    // Transfer from reward pool if available
                    let pool: i128 = env
                        .storage()
                        .instance()
                        .get(&DataKey::RewardPool)
                        .unwrap_or(0i128);

                    if pool >= reward_amount {
                        let token_addr: Address = env
                            .storage()
                            .instance()
                            .get(&DataKey::Token)
                            .expect("Token not set");
                        let client = token::Client::new(env, &token_addr);
                        client.transfer(
                            &env.current_contract_address(),
                            &goal.member,
                            &reward_amount,
                        );
                        env.storage().instance().set(&DataKey::RewardPool, &(pool - reward_amount));

                        crate::events::emit_milestone_reached(
                            env,
                            goal.group_id,
                            goal.member.clone(),
                            milestone.percentage,
                            reward_amount,
                        );
                    }
                    // If pool depleted: skip reward but mark milestone as claimed to avoid loops
                }

                // Mark milestone as claimed in bitmask
                let new_bitmask = bitmask | bit;
                env.storage().persistent().set(&bitmask_key, &new_bitmask);
                env.storage().persistent().extend_ttl(
                    &bitmask_key,
                    crate::PERSISTENT_LIFETIME_THRESHOLD,
                    crate::PERSISTENT_BUMP_AMOUNT,
                );
            }
        }
    }

    /// Get goal details
    pub fn get_goal(env: &Env, goal_id: u32) -> Option<SavingsGoal> {
        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        env.storage().persistent().get(&key)
    }

    /// Get goal progress
    pub fn get_goal_progress(env: &Env, goal_id: u32) -> GoalProgress {
        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        let goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::GoalNotFound));

        let now = env.ledger().timestamp();
        let percentage_completed = if goal.target_amount > 0 {
            ((goal.current_amount * 100) / goal.target_amount) as u32
        } else {
            0
        };

        let days_remaining = if goal.target_date > now {
            ((goal.target_date - now) / (24 * 60 * 60)) as i64
        } else {
            -1
        };

        let velocity = if now > goal.created_at {
            let days_elapsed = (now - goal.created_at) / (24 * 60 * 60);
            if days_elapsed > 0 {
                goal.current_amount / (days_elapsed as i128)
            } else {
                0
            }
        } else {
            0
        };

        GoalProgress {
            goal_id,
            current_amount: goal.current_amount,
            target_amount: goal.target_amount,
            percentage_completed,
            days_remaining,
            estimated_completion: goal.target_date,
            velocity,
            status: goal.status,
        }
    }

    /// Check and celebrate milestones
    pub fn check_and_celebrate_milestones(env: &Env, goal_id: u32) -> Vec<MilestoneCelebration> {
        let mut celebrations = Vec::new(env);

        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::GoalNotFound));

        let percentage_completed = if goal.target_amount > 0 {
            ((goal.current_amount * 100) / goal.target_amount) as u32
        } else {
            0
        };

        for milestone in goal.milestones.iter() {
            let mut already_completed = false;
            for completed_id in goal.completed_milestones.iter() {
                if completed_id == milestone.milestone_id {
                    already_completed = true;
                    break;
                }
            }

            if !already_completed && percentage_completed >= milestone.percentage {
                let celebration = MilestoneCelebration {
                    celebration_id: Self::get_next_celebration_id(env),
                    goal_id,
                    milestone_id: milestone.milestone_id,
                    member: goal.member.clone(),
                    timestamp: env.ledger().timestamp(),
                    celebration_type: CelebrationType::MilestoneReached,
                    message: String::from_str(env, "Milestone reached!"),
                    reward_issued: false,
                    reward_details: Map::new(env),
                    witnesses: Vec::new(env),
                };

                celebrations.push_back(celebration.clone());
                goal.completed_milestones.push_back(milestone.milestone_id);

                // Store celebration
                let celebration_key = (Symbol::new(env, CELEBRATION_KEY_PREFIX), celebration.celebration_id);
                env.storage().persistent().set(&celebration_key, &celebration);
            }
        }

        env.storage().persistent().set(&key, &goal);
        celebrations
    }

    /// Celebrate milestone manually
    pub fn celebrate_milestone(
        env: &Env,
        goal_id: u32,
        milestone_id: u32,
        message: String,
    ) -> MilestoneCelebration {
        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        let goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::GoalNotFound));

        goal.member.require_auth();

        let mut milestone_found = false;
        for milestone in goal.milestones.iter() {
            if milestone.milestone_id == milestone_id {
                milestone_found = true;
                break;
            }
        }

        if !milestone_found {
            panic_with_error!(env, SavingsGoalError::MilestoneNotFound);
        }

        let celebration_id = Self::get_next_celebration_id(env);

        let celebration = MilestoneCelebration {
            celebration_id,
            goal_id,
            milestone_id,
            member: goal.member.clone(),
            timestamp: env.ledger().timestamp(),
            celebration_type: CelebrationType::MilestoneReached,
            message,
            reward_issued: false,
            reward_details: Map::new(env),
            witnesses: Vec::new(env),
        };

        // Store celebration
        let celebration_key = (Symbol::new(env, CELEBRATION_KEY_PREFIX), celebration_id);
        env.storage().persistent().set(&celebration_key, &celebration);

        celebration
    }

    /// Issue reward for milestone
    pub fn issue_milestone_reward(
        env: &Env,
        celebration_id: u32,
        reward_details: Map<String, String>,
    ) {
        let celebration_key = (Symbol::new(env, CELEBRATION_KEY_PREFIX), celebration_id);
        let mut celebration: MilestoneCelebration = env
            .storage()
            .persistent()
            .get(&celebration_key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::InvalidContribution));

        celebration.reward_issued = true;
        celebration.reward_details = reward_details;

        env.storage().persistent().set(&celebration_key, &celebration);
    }

    /// Complete a goal
    pub fn complete_goal(env: &Env, goal_id: u32) -> MilestoneCelebration {
        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::GoalNotFound));

        goal.member.require_auth();

        if goal.status == GoalStatus::Completed {
            panic_with_error!(env, SavingsGoalError::GoalCompleted);
        }

        goal.status = GoalStatus::Completed;

        let celebration_id = Self::get_next_celebration_id(env);

        let celebration = MilestoneCelebration {
            celebration_id,
            goal_id,
            milestone_id: 0,
            member: goal.member.clone(),
            timestamp: env.ledger().timestamp(),
            celebration_type: CelebrationType::GoalCompleted,
            message: String::from_str(env, "Goal completed!"),
            reward_issued: false,
            reward_details: Map::new(env),
            witnesses: Vec::new(env),
        };

        // Store celebration
        let celebration_key = (Symbol::new(env, CELEBRATION_KEY_PREFIX), celebration_id);
        env.storage().persistent().set(&celebration_key, &celebration);

        // Store updated goal
        env.storage().persistent().set(&key, &goal);

        celebration
    }

    /// Pause a goal
    pub fn pause_goal(env: &Env, goal_id: u32) {
        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::GoalNotFound));

        goal.member.require_auth();

        goal.status = GoalStatus::Paused;
        env.storage().persistent().set(&key, &goal);
    }

    /// Resume a paused goal
    pub fn resume_goal(env: &Env, goal_id: u32) {
        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::GoalNotFound));

        goal.member.require_auth();

        if goal.status != GoalStatus::Paused {
            panic_with_error!(env, SavingsGoalError::InvalidGoalStatus);
        }

        goal.status = GoalStatus::Active;
        env.storage().persistent().set(&key, &goal);
    }

    /// Abandon a goal
    pub fn abandon_goal(env: &Env, goal_id: u32) {
        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::GoalNotFound));

        goal.member.require_auth();

        goal.status = GoalStatus::Abandoned;
        env.storage().persistent().set(&key, &goal);
    }

    /// Get member's goals
    pub fn get_member_goals(env: &Env, member: Address) -> Vec<SavingsGoal> {
        let mut goals = Vec::new(env);
        let member_key = (Symbol::new(env, MEMBER_GOALS_KEY_PREFIX), member.clone());

        if let Some(goal_ids) = env.storage().persistent().get::<_, Vec<u32>>(&member_key) {
            for id in goal_ids.iter() {
                let key = (Symbol::new(env, GOAL_KEY_PREFIX), id);
                if let Some(goal) = env.storage().persistent().get::<_, SavingsGoal>(&key) {
                    goals.push_back(goal);
                }
            }
        }

        goals
    }

    /// Get group goals summary
    pub fn get_group_goals_summary(env: &Env, group_id: u32) -> GroupGoalSummary {
        let mut total_goals = 0u32;
        let mut completed_goals = 0u32;
        let mut active_goals = 0u32;
        let mut total_saved: i128 = 0;
        let mut total_target: i128 = 0;
        let mut total_percentage: u32 = 0;

        let group_key = (Symbol::new(env, GROUP_GOALS_KEY_PREFIX), group_id);

        if let Some(goal_ids) = env.storage().persistent().get::<_, Vec<u32>>(&group_key) {
            for id in goal_ids.iter() {
                let key = (Symbol::new(env, GOAL_KEY_PREFIX), id);
                if let Some(goal) = env.storage().persistent().get::<_, SavingsGoal>(&key) {
                    total_goals = total_goals.saturating_add(1);
                    total_saved = total_saved.saturating_add(goal.current_amount);
                    total_target = total_target.saturating_add(goal.target_amount);

                    match goal.status {
                        GoalStatus::Completed => {
                            completed_goals = completed_goals.saturating_add(1);
                            total_percentage = total_percentage.saturating_add(100);
                        }
                        GoalStatus::Active => {
                            active_goals = active_goals.saturating_add(1);
                            let percentage = if goal.target_amount > 0 {
                                ((goal.current_amount * 100) / goal.target_amount) as u32
                            } else {
                                0
                            };
                            total_percentage = total_percentage.saturating_add(percentage);
                        }
                        _ => {}
                    }
                }
            }
        }

        let avg_completion_percentage = if total_goals > 0 {
            total_percentage / total_goals
        } else {
            0
        };

        GroupGoalSummary {
            group_id,
            total_goals,
            completed_goals,
            active_goals,
            total_saved,
            total_target,
            avg_completion_percentage,
            top_contributors: Vec::new(env),
        }
    }

    /// Get goal contributions
    pub fn get_goal_contributions(env: &Env, goal_id: u32) -> Vec<GoalContribution> {
        let mut contributions = Vec::new(env);
        // In production, use proper indexing to retrieve contributions for this goal
        contributions
    }

    /// Get milestone celebrations
    pub fn get_milestone_celebrations(env: &Env, goal_id: u32) -> Vec<MilestoneCelebration> {
        let mut celebrations = Vec::new(env);
        // In production, use proper indexing to retrieve celebrations for this goal
        celebrations
    }

    /// Issue achievement badge
    pub fn issue_achievement_badge(
        env: &Env,
        member: Address,
        badge_type: BadgeType,
        metadata: Map<String, String>,
    ) -> GoalAchievementBadge {
        let badge_id = Self::get_next_badge_id(env);

        let badge = GoalAchievementBadge {
            badge_id,
            member,
            badge_type,
            issued_at: env.ledger().timestamp(),
            metadata,
        };

        // Store badge
        let key = (Symbol::new(env, BADGE_KEY_PREFIX), badge_id);
        env.storage().persistent().set(&key, &badge);

        badge
    }

    /// Get member badges
    pub fn get_member_badges(env: &Env, member: Address) -> Vec<GoalAchievementBadge> {
        let mut badges = Vec::new(env);
        // In production, use proper indexing to retrieve badges for this member
        badges
    }

    /// Get celebration leaderboard
    pub fn get_celebration_leaderboard(env: &Env, group_id: u32) -> Vec<(Address, u32)> {
        let mut leaderboard = Vec::new(env);
        // In production, use proper indexing to build leaderboard
        leaderboard
    }

    /// Update goal metadata
    pub fn update_goal_metadata(
        env: &Env,
        goal_id: u32,
        metadata: Map<String, String>,
    ) {
        let key = (Symbol::new(env, GOAL_KEY_PREFIX), goal_id);
        let mut goal: SavingsGoal = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SavingsGoalError::GoalNotFound));

        goal.member.require_auth();

        goal.metadata = metadata;
        env.storage().persistent().set(&key, &goal);
    }

    /// Get goals by category
    pub fn get_goals_by_category(
        env: &Env,
        group_id: u32,
        category: String,
    ) -> Vec<SavingsGoal> {
        let mut goals = Vec::new(env);
        let group_key = (Symbol::new(env, GROUP_GOALS_KEY_PREFIX), group_id);

        if let Some(goal_ids) = env.storage().persistent().get::<_, Vec<u32>>(&group_key) {
            for id in goal_ids.iter() {
                let key = (Symbol::new(env, GOAL_KEY_PREFIX), id);
                if let Some(goal) = env.storage().persistent().get::<_, SavingsGoal>(&key) {
                    if goal.category == category {
                        goals.push_back(goal);
                    }
                }
            }
        }

        goals
    }

    /// Get top goal contributors
    pub fn get_top_goal_contributors(
        env: &Env,
        group_id: u32,
        limit: u32,
    ) -> Vec<(Address, i128)> {
        let mut contributors = Vec::new(env);
        // In production, use proper indexing to retrieve top contributors
        contributors
    }

    /// Helper function to get next celebration ID
    fn get_next_celebration_id(env: &Env) -> u32 {
        let celebration_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, CELEBRATION_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_id = celebration_id.checked_add(1).unwrap_or(celebration_id);
        env.storage()
            .instance()
            .set(&Symbol::new(env, CELEBRATION_COUNTER_KEY), &next_id);

        next_id
    }

    /// Helper function to get next badge ID
    fn get_next_badge_id(env: &Env) -> u32 {
        let badge_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, BADGE_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_id = badge_id.checked_add(1).unwrap_or(badge_id);
        env.storage()
            .instance()
            .set(&Symbol::new(env, BADGE_COUNTER_KEY), &next_id);

        next_id
    }
}
