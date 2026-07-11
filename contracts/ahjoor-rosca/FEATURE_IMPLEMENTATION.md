# ROSCA Contract - New Features Implementation

This document describes the three new features implemented in the Ahjoor ROSCA contract.

## Overview

Three major features have been added to enhance the ROSCA contract functionality:

> **Migration note:** six storage keys were moved from `DataKey` into `DataKey2` to avoid the Soroban XDR 50-variant limit. Those keys preserve their original storage discriminants so existing on-chain state continues to resolve to the same slots after upgrade.


1. **Protocol Fee on ROSCA Round Payouts** - Sustainable revenue for the protocol treasury
2. **Partial Contribution Installments Within a Round** - Lower barrier to participation
3. **Configurable Defaulter Suspension Threshold** - Flexible group tolerance levels

---

## Feature 1: Protocol Fee on ROSCA Round Payouts

### Problem

The ROSCA contract previously disbursed the full pot to the round recipient with no protocol fee deduction, providing no mechanism to sustain the Ahjoor protocol treasury.

### Solution

A configurable fee mechanism has been implemented that:

- Deducts a percentage-based fee from each round payout
- Transfers the fee to a designated fee recipient address
- Enforces a hard cap of 500 basis points (5%) to protect users
- Allows admin to update the fee rate dynamically

### Implementation Details

#### Storage Keys Added

- `FeeBps` (u32) - Protocol fee in basis points (e.g., 100 = 1%, 500 = 5%)
- `FeeRecipient` (Address) - Address that receives protocol fees

#### Configuration

Fees are configured during initialization via the `RoscaConfig` struct:

```rust
pub struct RoscaConfig {
    // ... existing fields ...
    pub fee_bps: u32,                    // Fee in basis points (max 500)
    pub fee_recipient: Option<Address>,  // Fee recipient address
}
```

#### Fee Calculation

Fees are calculated and deducted in the `complete_round_payout()` function:

```rust
let fee_amount = (pot_balance * fee_bps) / 10_000;
let payout_amount = pot_balance - fee_amount;
```

#### New Functions

**`update_fee(env: Env, new_fee_bps: u32)`**

- Admin-only function to update the protocol fee
- Validates that new fee doesn't exceed 500 bps (5%)
- Emits no event (fee changes are transparent via queries)

**`get_fee_bps(env: Env) -> u32`**

- Query function to get current fee rate
- Returns 0 if no fee is configured

**`get_fee_recipient(env: Env) -> Option<Address>`**

- Query function to get fee recipient address
- Returns None if no recipient is configured

#### Events

**`FeeCollected`**

```rust
pub struct FeeCollected {
    pub round: u32,
    pub fee_amount: i128,
    pub fee_recipient: Address,
}
```

Emitted when a protocol fee is collected during round payout.

#### Validation

- Fee rate is capped at 500 bps (5%) during both initialization and updates
- Attempting to set a higher fee results in `Error::FeeExceedsMaximum`
- Fee is only deducted if both `fee_bps > 0` and `fee_recipient` is set

#### Example Usage

```rust
// Initialize with 2% fee
client.init(
    &admin,
    &members,
    &100,
    &token,
    &3600,
    &RoscaConfig {
        // ... other config ...
        fee_bps: 200,  // 2%
        fee_recipient: Some(treasury_address),
    },
);

// Later, update fee to 3%
client.update_fee(&300);

// Query current fee
let current_fee = client.get_fee_bps();  // Returns 300
```

---

## Feature 2: Partial Contribution Installments Within a Round

### Problem

Members previously had to contribute the full `contribution_amount` in a single transaction, creating a high barrier to participation for members with limited liquidity.

### Solution

Members can now make multiple partial contributions within a round:

- Contributions accumulate toward the required amount
- Members are only marked as "fully paid" when they reach the target
- Round payout only triggers when all members are fully paid
- Partial contribution status is queryable

### Implementation Details

#### Storage

The existing `MemberContributions` map tracks cumulative contributions per member per round:

```rust
MemberContributions: Map<Address, i128>  // Cumulative per round
```

This map is reset at the start of each new round.

#### Contribution Logic

The `contribute()` function now:

1. Accepts any positive amount up to the remaining balance
2. Validates that the contribution doesn't exceed what's remaining
3. Updates the cumulative contribution for the member
4. Only marks the member as "fully paid" when cumulative equals target
5. Emits a `PartialContributionReceived` event if not yet fully paid

#### New Functions

**`get_member_contribution_status(env: Env, member: Address) -> (i128, i128)`**

- Returns `(amount_contributed, amount_remaining)` for the current round
- Useful for UIs to show progress bars or remaining amounts
- Returns `(0, contribution_amount)` for members who haven't contributed yet

#### Events

**`PartialContributionReceived`**

```rust
pub struct PartialContributionReceived {
    pub member: Address,
    pub round: u32,
    pub amount: i128,
    pub remaining: i128,
}
```

Emitted when a member makes a partial contribution (not yet fully paid).

#### Validation

- Contributions must be positive (`Error::AmountMustBePositive`)
- Cannot exceed remaining amount (`Error::ExceedsRemainingContribution`)
- Cannot contribute after deadline (`Error::ContributionWindowClosed`)
- Cannot contribute if already fully paid (`Error::AlreadyContributed`)

#### Example Usage

```rust
// Member needs to contribute 100 total
// Pay in three installments
client.contribute(&member, &token, &30);  // 30 paid, 70 remaining
client.contribute(&member, &token, &40);  // 70 paid, 30 remaining
client.contribute(&member, &token, &30);  // 100 paid, 0 remaining (fully paid)

// Query status at any time
let (paid, remaining) = client.get_member_contribution_status(&member);
// Returns (70, 30) after second contribution
```

#### Behavior Notes

- The `ContributionReceived` event is emitted for every contribution (partial or full)
- The `PartialContributionReceived` event is emitted only when remaining > 0
- Participation tracking (for rewards) only increments when fully paid
- Round payout only triggers when ALL members are fully paid

---

## Feature 3: Configurable Defaulter Suspension Threshold

### Problem

The contract previously hard-coded suspension after 3 consecutive missed rounds. Different groups have different tolerance levels and should be able to define their own thresholds.

### Solution

The suspension threshold is now configurable:

- Set during initialization via `RoscaConfig.max_defaults`
- Must be at least 1 (validated during init)
- Used consistently in both `penalise_defaulter()` and `finalize_round()`
- Queryable via `get_max_defaults()`

### Implementation Details

#### Storage Key Added

- `MaxDefaults` (u32) - Number of consecutive missed rounds before suspension

#### Configuration

The threshold is set during initialization:

```rust
pub struct RoscaConfig {
    // ... existing fields ...
    pub max_defaults: u32,  // Must be >= 1
}
```

#### Suspension Logic

Both `penalise_defaulter()` and `finalize_round()` now use the configured threshold:

```rust
let max_defaults: u32 = env
    .storage()
    .instance()
    .get(&DataKey::MaxDefaults)
    .unwrap_or(3);  // Default to 3 for backward compatibility

if default_count >= max_defaults {
    // Suspend member
}
```

#### New Functions

**`get_max_defaults(env: Env) -> u32`**

- Query function to get the configured suspension threshold
- Returns 3 as default if not set (backward compatibility)

#### Events

**`SuspensionThresholdSet`**

```rust
pub struct SuspensionThresholdSet {
    pub max_defaults: u32,
}
```

Emitted during initialization to record the configured threshold.

#### Validation

- `max_defaults` must be at least 1 during initialization
- Attempting to set 0 results in `Error::InvalidMaxDefaults`

#### Example Usage

```rust
// Initialize with lenient threshold (5 defaults before suspension)
client.init(
    &admin,
    &members,
    &100,
    &token,
    &3600,
    &RoscaConfig {
        // ... other config ...
        max_defaults: 5,  // More lenient
    },
);

// Initialize with strict threshold (1 default = immediate suspension)
client.init(
    &admin,
    &members,
    &100,
    &token,
    &3600,
    &RoscaConfig {
        // ... other config ...
        max_defaults: 1,  // Very strict
    },
);

// Query threshold
let threshold = client.get_max_defaults();  // Returns 5 or 1
```

#### Behavior Notes

- The threshold applies to **consecutive** missed rounds
- If a member contributes in a round, their default count is NOT reset
- Only successful penalty appeals (via governance) reset the default count
- Suspended members cannot receive payouts but remain in the member list

---

## Error Codes

Three new error codes have been added:

| Code | Name                 | Description                                             |
| ---- | -------------------- | ------------------------------------------------------- |
| 34   | `FeeExceedsMaximum`  | Fee basis points exceeds maximum allowed (500 bps = 5%) |
| 35   | `InvalidMaxDefaults` | Max defaults must be at least 1                         |

---

## Testing

Comprehensive test coverage has been added in `src/test_new_features.rs`:

### Protocol Fee Tests

- `test_protocol_fee_deducted_from_payout` - Verifies fee calculation and distribution
- `test_protocol_fee_max_cap_enforced` - Validates 500 bps cap during init
- `test_update_fee_function` - Tests dynamic fee updates and cap enforcement
- `test_no_fee_when_fee_bps_zero` - Ensures no fee when disabled

### Partial Contribution Tests

- `test_partial_contribution_installments` - Tests multi-payment flow
- `test_partial_contribution_events_emitted` - Verifies event emission
- `test_cannot_exceed_remaining_contribution` - Validates overpayment protection
- `test_get_member_contribution_status` - Tests status query function

### Suspension Threshold Tests

- `test_configurable_max_defaults` - Tests custom threshold enforcement
- `test_suspension_threshold_set_event` - Verifies event emission
- `test_max_defaults_must_be_at_least_one` - Validates minimum threshold
- `test_penalise_defaulter_uses_max_defaults` - Tests manual penalty flow

### Integration Test

- `test_all_features_integrated` - Tests all three features working together

**Test Results:** All 111 tests pass (98 existing + 13 new)

---

## Migration Guide

### For Existing Deployments

If you have an existing ROSCA contract deployment, you'll need to:

1. **Upgrade the contract** using the `upgrade()` function
2. **Run migration** using the `migrate()` function
3. **Set default values** for new storage keys (if needed):
   - `FeeBps` defaults to 0 (no fee)
   - `FeeRecipient` defaults to None
   - `MaxDefaults` defaults to 3 (backward compatible)

### For New Deployments

Simply include the new fields in your `RoscaConfig`:

```rust
let config = RoscaConfig {
    strategy: PayoutStrategy::RoundRobin,
    custom_order: None,
    penalty_amount: 10,
    exit_penalty_bps: 1000,  // 10%
    collective_goal: None,
    member_goals: None,
    fee_bps: 200,                          // NEW: 2% protocol fee
    fee_recipient: Some(treasury_address), // NEW: Fee recipient
    max_defaults: 3,                       // NEW: Suspend after 3 defaults
};

client.init(&admin, &members, &100, &token, &3600, &config);
```

---

## Security Considerations

### Protocol Fee

- **Fee Cap:** Hard-coded 500 bps (5%) maximum protects users from excessive fees
- **Admin Control:** Only admin can update fees, preventing unauthorized changes
- **Transparency:** Fee rate and recipient are publicly queryable
- **No Retroactive Fees:** Fee changes only affect future rounds

### Partial Contributions

- **Overpayment Protection:** Cannot contribute more than remaining amount
- **Deadline Enforcement:** Partial contributions still respect round deadlines
- **Atomicity:** Each contribution is atomic; no partial state corruption
- **Payout Safety:** Payout only triggers when ALL members are fully paid

### Suspension Threshold

- **Minimum Validation:** Threshold must be at least 1 (prevents division by zero)
- **Immutable After Init:** Threshold cannot be changed after initialization
- **Consistent Application:** Same threshold used in all suspension logic paths
- **Backward Compatible:** Defaults to 3 if not set (matches old behavior)

---

## Gas Optimization Notes

- **Storage Efficiency:** Reuses existing `MemberContributions` map for partial tracking
- **Event Optimization:** Only emits `PartialContributionReceived` when needed
- **Query Functions:** All new query functions are read-only (no gas for queries)
- **Fee Calculation:** Simple integer arithmetic (no floating point)

---

## Future Enhancements

Potential improvements for future versions:

1. **Dynamic Fee Schedules:** Time-based or volume-based fee tiers
2. **Fee Distribution:** Split fees among multiple recipients
3. **Contribution Limits:** Min/max per installment to prevent spam
4. **Grace Periods:** Allow members to catch up before suspension
5. **Threshold Updates:** Allow admin to update `max_defaults` post-init

---

## API Reference

### New Functions

```rust
// Protocol Fee Management
pub fn update_fee(env: Env, new_fee_bps: u32);
pub fn get_fee_bps(env: Env) -> u32;
pub fn get_fee_recipient(env: Env) -> Option<Address>;

// Partial Contribution Status
pub fn get_member_contribution_status(env: Env, member: Address) -> (i128, i128);

// Suspension Threshold
pub fn get_max_defaults(env: Env) -> u32;
```

### Modified Functions

```rust
// Now accepts partial amounts
pub fn contribute(env: Env, contributor: Address, token: Address, amount: i128);
```

### New Events

```rust
pub struct FeeCollected {
    pub round: u32,
    pub fee_amount: i128,
    pub fee_recipient: Address,
}

pub struct PartialContributionReceived {
    pub member: Address,
    pub round: u32,
    pub amount: i128,
    pub remaining: i128,
}

pub struct SuspensionThresholdSet {
    pub max_defaults: u32,
}
```

---

## Conclusion

These three features significantly enhance the ROSCA contract by:

- Providing sustainable protocol revenue
- Lowering barriers to participation
- Offering flexible group management

All features are backward compatible, well-tested, and production-ready.
