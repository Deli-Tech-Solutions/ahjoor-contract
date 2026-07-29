# Escrow Dispute Flow

This document describes the dispute-resolution flow used by the `ahjoor-escrow` contract (`contracts/ahjoor-escrow`), including **percentage-split verdicts** (#4).

Summary

- Default dispute timeout: 7 days (604,800 seconds).
- Per-escrow override: `create_escrow_w_timeout(..., dispute_timeout_seconds)`.
- Verdicts are **not** binary buyer/seller — an arbiter rules with a `buyer_percent` (0–100) split; the seller receives the remainder. 100 = full buyer win, 0 = full seller win, anything in between is a proportional split.
- Status transitions: `Active` → `Disputed`/`PartiallyDisputed` → (`CoolingOff` →) `Resolved`/`Refunded`/`Released`.

Step-by-step flow

1. Buyer or seller calls `dispute_escrow(caller, escrow_id, reason, dispute_amount)`.
   - `dispute_amount` may equal the full escrow amount (full dispute → status `Disputed`) or less (partial dispute → the undisputed remainder is released to the seller immediately and status becomes `PartiallyDisputed`). Either way `escrow.amount` is reduced to the disputed portion, and a dispute deadline is recorded: `now + dispute_timeout_seconds` (or the 604,800s default).

2. The `arbiter` is fixed at escrow creation time (`create_escrow(..., arbiter, ...)` / `create_escrow_w_timeout(..., arbiter, ...)`) — there is no separate assignment step.

3. Arbiter calls `resolve_dispute(arbiter, escrow_id, buyer_percent)` before the deadline.
   - `buyer_percent` must be 0–100; the seller's share is `100 - buyer_percent`.
   - If no cooling-off window is configured (`ResolutionCoolingOffSeconds == 0`), the verdict executes immediately: protocol fee and arbiter fee are deducted from the disputed amount, the remainder is split `buyer_percent` / `seller_percent` between the parties, and any seller collateral is forfeited proportionally (see "Collateral interaction" below). Status becomes `Refunded` (100/0), `Released` (0/100), or `Resolved` (any split in between).
   - If a cooling-off window **is** configured, the verdict is recorded but funds are **not** moved yet — see step 4.

4. Cooling-off window (optional, admin-configured via `set_resolution_cooloff_secs(admin, seconds)`).
   - `resolve_dispute` records a `PendingVerdict { buyer_percent, arbiter, recorded_at }` and moves the escrow to `CoolingOff` instead of transferring funds.
   - Either the buyer or seller may call `flag_resolution_error(caller, escrow_id, reason_hash)` during the window to pause the release and escalate to the admin queue (only once per verdict; must be within the window).
   - If unflagged, anyone can call `finalize_resolution(escrow_id)` once the window elapses — this executes the exact same split logic as the immediate path in step 3.
   - If flagged, the admin reviews and calls `clear_resolution_flag(admin, escrow_id)` to unblock `finalize_resolution`.

5. If the arbiter misses the dispute deadline entirely (never called `resolve_dispute`), anyone calls `enforce_dispute_timeout(escrow_id)`.
   - This is a **binary** fallback, not a split: the full remaining `escrow.amount` goes to the configured default winner (`DisputeDefaultWinner::Buyer` or `::Seller`, per-escrow override or the admin-wide default from `set_default_dispute_winner`). Because there was no arbiter ruling, there is no split to apply, and this path currently does **not** deduct protocol/arbiter fees or touch collateral — it is strictly the "nobody ruled, return to a safe default" escape hatch, distinct from `resolve_dispute`'s split-aware settlement.
   - Increments the arbiter's timeout counter (`get_arbiter_timeout_count`), used to track/penalize inactive arbiters.
   - A partial dispute that later times out is unaffected by the split feature: `enforce_dispute_timeout` operates on whatever `escrow.amount` remains after the partial release in step 1.

Verdict shape

```text
resolve_dispute(arbiter: Address, escrow_id: u32, buyer_percent: u32)
```

- `buyer_percent` — integer 0–100. Panics with `"buyer_percent must be between 0 and 100"` outside that range.
- Seller share is implicitly `100 - buyer_percent`; the two are not passed separately, so they can never disagree with each other.
- Fee deduction order (both the immediate and cooling-off-finalized paths): arbiter fee → protocol fee → the remainder is split `buyer_percent`/`seller_percent`.
- Events: `DisputeResolvedSplit { escrow_id, buyer_percent, seller_percent, buyer_amount, seller_amount, arbiter }` carries the full split; `DisputeResolved { escrow_id, release_to_seller, arbiter }` and `ResolutionFinalized { escrow_id, buyer_percent, arbiter }` are also emitted for backward-compatible binary-style consumers (`release_to_seller` is `true` only for a full 0% buyer_percent seller win).

Collateral interaction (#237)

If the escrow was created with seller collateral (`create_multi_seller_escrow(..., required_collateral_bps, collateral_forfeit_bps, ...)` + `deposit_collateral`), a dispute verdict handles it based on whether the buyer received *any* share:

- `buyer_percent > 0` (buyer favoured, even partially): `collateral_forfeit_bps` of the collateral is forfeited to the buyer (`CollateralForfeited` event), the remainder returned to the seller (`CollateralReturned`).
- `buyer_percent == 0` (full seller win): the full collateral is returned to the seller.

This applies identically whether the verdict came from the immediate path or `finalize_resolution` after cooling off.

Notes and implementation details

- **Default timeout**: 604,800 seconds (7 days), via `DEFAULT_DISPUTE_TIMEOUT_SECONDS`. Override per-escrow with `create_escrow_w_timeout(...)`, or admin-wide with `update_default_dispute_timeout(admin, seconds)`.
- **Status transitions**:
  - `Active` — normal escrow life before dispute.
  - `Disputed` / `PartiallyDisputed` — raised via `dispute_escrow`; a deadline is set.
  - `CoolingOff` — arbiter has ruled but the cooling-off window hasn't elapsed (or was flagged); funds are held.
  - `Resolved` — a split verdict (`0 < buyer_percent < 100`) executed.
  - `Refunded` — `buyer_percent == 100` (full buyer win).
  - `Released` — `buyer_percent == 0` (full seller win).
- **Arbiter timeout counter**: incremented on every `enforce_dispute_timeout` call, usable by off-chain governance/admin logic to suspend or penalize repeatedly inactive arbiters.
- **Inspector reputation** (#357): every executed verdict (immediate or cooling-off-finalized) records a ruling against the escrow's inspector, if one is set, for reputation scoring — split verdicts count the same as binary ones.

Examples

- Split verdict with default (no cooling-off):
  1. Buyer calls `dispute_escrow(buyer, escrow_id, "quality issue", 1000)` on a 1000-unit escrow → full dispute, `dispute_deadline = now + 604800`.
  2. Arbiter calls `resolve_dispute(arbiter, escrow_id, 60)` → buyer receives 60% of the (fee-adjusted) amount, seller receives 40%; status becomes `Resolved`.

- Split verdict with cooling-off:
  1. Admin calls `set_resolution_cooloff_secs(admin, 86400)` (24h cooling-off).
  2. Arbiter calls `resolve_dispute(arbiter, escrow_id, 30)` → escrow enters `CoolingOff`; no funds move yet.
  3a. No flag raised: after 24h, anyone calls `finalize_resolution(escrow_id)` → 30%/70% split executes.
  3b. Flag raised: buyer or seller calls `flag_resolution_error(caller, escrow_id, reason_hash)` within the window → admin reviews and calls `clear_resolution_flag(admin, escrow_id)` before `finalize_resolution` can proceed.

- Arbiter timeout (binary fallback, no split):
  1. Buyer disputes; arbiter never rules.
  2. After the deadline passes, anyone calls `enforce_dispute_timeout(escrow_id)` → full remaining amount goes to the configured default winner; arbiter's timeout counter increments.

CLI examples

```bash
# Arbiter resolves a dispute with a 60/40 buyer/seller split
stellar contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  --source arbiter \
  --network testnet \
  -- resolve_dispute --arbiter <ARBITER_ADDRESS> --escrow_id 42 --buyer_percent 60

# Admin configures a 24h cooling-off window before verdicts finalize
stellar contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- set_resolution_cooloff_secs --admin <ADMIN_ADDRESS> --seconds 86400

# Anyone finalizes a cooling-off verdict once the window has elapsed
stellar contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  --network testnet \
  -- finalize_resolution --escrow_id 42

# Anyone enforces the binary default-winner fallback after an arbiter timeout
stellar contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  --network testnet \
  -- enforce_dispute_timeout --escrow_id 42
```

Make sure to consider these points when integrating with frontends or off-chain tooling:

- Show a clear dispute state, the current `buyer_percent`/`seller_percent` split once ruled, and a countdown until `dispute_deadline` (and, if applicable, the cooling-off window's end).
- Surface arbiter identity and a link to arbiter reputation or timeout count.
- Distinguish the arbiter's split verdict path from the binary `enforce_dispute_timeout` fallback in the UI — they have different fee/collateral handling.
