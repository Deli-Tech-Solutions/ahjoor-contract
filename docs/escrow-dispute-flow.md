# Escrow Dispute Flow

This document describes the multi-step dispute flow used by the ahjoor-escrow contract.

Summary

- Default timeout: 7 days (604,800 seconds).
- Per-escrow override: `create_escrow_w_timeout( ... , timeout_seconds )`.
- Status transitions: `Active` → `Disputed` → `Resolved` / `TimedOut`.

Step-by-step flow

1. Buyer calls `dispute_escrow(escrow_id)`
   - Marks the escrow as `Disputed` and records a dispute deadline: `now + escrow.timeout_seconds` (or default 604,800s).
   - Starts the dispute timer; arbiter must be assigned and act before the deadline.

2. Admin assigns an arbiter via `assign_arbiter(escrow_id, arbiter)`
   - Sets the `arbiter` for the escrow. The arbiter is responsible for resolving the dispute.

3. Arbiter calls `resolve_dispute(escrow_id, winner)` before the deadline
   - If the arbiter resolves in time, the contract transfers funds to the declared `winner` and sets status `Resolved`.

4. If arbiter misses deadline → anyone calls `enforce_dispute_timeout(escrow_id)`
   - If current time > dispute deadline and escrow still `Disputed`, this call:
     - Releases funds to the `default winner` (as defined by contract logic or dispute invocation parameters).
     - Marks escrow as `TimedOut` (a terminal state similar to `Resolved`).
     - Increments the arbiter's timeout counter (used for tracking arbiter inactivity/misbehavior).

Notes and implementation details

- Default Timeout: The contract uses a default of 604_800 seconds (7 days). When `dispute_escrow` is called and no per-escrow timeout is set, the dispute deadline is computed using that default.
- Per-escrow Override: Use `create_escrow_w_timeout(...)` to create an escrow whose `timeout_seconds` differs from the default. This value is persisted on the escrow and used for its dispute deadline.
- Status Transitions:
  - `Active` — normal escrow life before dispute.
  - `Disputed` — raised by the buyer calling `dispute_escrow`; a deadline is set.
  - `Resolved` — arbiter resolved the dispute before the deadline and funds were distributed accordingly.
  - `TimedOut` — arbiter failed to resolve before deadline; `enforce_dispute_timeout` was called and funds were distributed to the default winner; arbiter timeout counter incremented.

- Arbiter timeout counter: Whenever a dispute is forced via `enforce_dispute_timeout` because the arbiter missed the deadline, the contract increments a per-arbiter counter. This counter can be used by off-chain governance or admin logic to suspend or penalize repeatedly inactive arbiters.

Examples

- Typical flow with default timeout:
  1. Buyer calls `dispute_escrow(escrow_id)` → `dispute_deadline = now + 604800`.
  2. Admin calls `assign_arbiter(escrow_id, arbiter_addr)`.
  3. If arbiter calls `resolve_dispute(escrow_id, buyer)` before `dispute_deadline`, escrow becomes `Resolved` and funds go to `buyer`.
  4. If arbiter does not act and `dispute_deadline` passes, any caller can call `enforce_dispute_timeout(escrow_id)` to release funds and increment arbiter timeout counter.

Make sure to consider these points when integrating with frontends or off-chain tooling:

- Show a clear dispute state and a countdown until `dispute_deadline` to users.
- Surface arbiter identity and a link to arbiter reputation or timeout count.
- Allow admins to create escrows with shorter/longer `timeout_seconds` via `create_escrow_w_timeout` for special cases.

If you need this doc expanded with code snippets from the contract (field names, exact method signatures), tell me which file you'd like referenced and I will extract and include the precise signatures.
