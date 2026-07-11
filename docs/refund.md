# Refund Contract (ahjoor-refund)

This document describes the refund contract (`contracts/ahjoor-refund`) behavior, who can trigger refunds, and the typical flow for requesting/approving/claiming refunds.

## When refunds apply
- Cancelled escrow: when an escrow is cancelled before funds are released.
- Failed round: if a ROSCA round fails to execute (e.g., not enough contributions).
- Overpayment: participant deposited more than required or duplicate payments.

Refund issuance is typically created when an upstream contract (e.g., `ahjoor-escrow` or `ahjoor-rosca`) determines funds must be returned. The refund record may be created automatically by the originating contract or by an explicit call to the refund contract.

## Who can trigger a refund
- Admin: can create or approve refunds in exceptional cases or to resolve disputes.
- Participant: a participant can request a refund for their own payment (see `request_refund`).
- Automatic: originating contracts may create refund records automatically on cancellation or failure.

## Key functions

- `request_refund(refund_id)` — participant requests a refund for a specific payment or escrow. Creates a refund record in `Pending` status when caller is the beneficiary.

- `approve_refund(refund_id)` — admin approves a pending refund. Moves refund to `Approved` state and records timestamp and approved amount. Only callable by admin or an authorized arbiter.

- `claim_refund(refund_id)` — participant claims funds for an approved refund. Transfers funds to claimant and marks refund `Claimed`.

- `create_refund(refund_id, owner, amount, metadata)` — (internal / called by originating contracts) create a refund record (used for automatic creation on cancellation).

- `get_refund(refund_id) -> Refund` — view refund record and status (`Pending`, `Approved`, `Claimed`, `Cancelled`).

## Typical flow

1. Escrow is cancelled (or round fails / overpayment detected).
2. Originating contract calls `create_refund(...)` on `ahjoor-refund` (automatic) OR participant calls `request_refund(refund_id)`.
3. Admin (or arbiter) reviews and calls `approve_refund(refund_id)` to approve the refund.
4. Participant calls `claim_refund(refund_id)` to withdraw the approved amount.

Alternate shorter flow (automatic approval): some flows can be configured so that refund creation includes an initial `Approved` status, allowing `claim_refund` directly after creation.

## Time limits and expirations

- Approval-to-claim window: the contract may enforce a time window (e.g., 30 days) within which the claimant must call `claim_refund` after approval. After the window expires the refund may be moved to `Expired` and require admin re-approval.
- Claim timelock: some refunds may include an optional timelock preventing claims until a given epoch (useful for dispute cooling-off).

Check the contract configuration constants for the exact timeouts used in the deployed instance.

## Events and error handling

- Events: `RefundRequested`, `RefundCreated`, `RefundApproved`, `RefundClaimed`, `RefundExpired`, `RefundCancelled`.
- Common errors: `NotAuthorized`, `InvalidRefundState`, `RefundNotFound`, `ClaimWindowExpired`, `InsufficientBalance`.

## Example (CLI)

Request a refund (participant):

```bash
stellar contract invoke --id <REFUND_CONTRACT_ID> --network testnet -- request_refund --refund-id <ID>
```

Approve (admin):

```bash
stellar contract invoke --id <REFUND_CONTRACT_ID> --network testnet -- approve_refund --refund-id <ID>
```

Claim (participant):

```bash
stellar contract invoke --id <REFUND_CONTRACT_ID> --network testnet -- claim_refund --refund-id <ID>
```

## Notes for integrators

- Originating contracts should set refund `owner` and `amount` precisely to avoid disputes.
- Admin approvals should be auditable — consider storing `approver` and `approved_at` on the refund record.
- If automatic approvals are enabled, ensure checks are in place to prevent double refunds.

---

See `contracts/ahjoor-refund` for on-chain implementation details and exact function signatures.
