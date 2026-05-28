# Escrow Dual-Custody Release — Implementation Notes

## Goal

Add an **optional dual-custody release mode** to `ahjoor-escrow` such that fund release requires **both**:

1. the **buyer**, and
2. a **designated co-signer** (e.g., corporate approver, legal counsel, DAO multisig delegate).

This prevents unilateral release in governance-sensitive workflows.

## Proposed External Behavior (Acceptance Criteria)

### 1) Creation

- Extend `create_escrow` to accept an optional `co_signer: Option<Address>`.
- If `co_signer` is provided, escrow enters **dual-custody mode**.

### 2) Dual Authorization Flow

- Buyer calls `authorize_release(escrow_id)`:
  - records buyer intent
  - escrow status transitions to **BuyerAuthorized**
- Co-signer calls `cosign_release(escrow_id)`:
  - records co-signer approval
  - when **both** authorizations are present, release is executed **atomically** to the seller.

### 3) Revocation

- Either party can call `revoke_release_authorization(escrow_id)`:
  - allowed **before** the second authorization arrives
  - resets escrow back to **Active**
- Revocation is blocked **after** both parties have authorized.

### 4) Co-signer Update

- `update_cosigner(escrow_id, new_cosigner)` is allowed:
  - only **before** any release authorization is in progress.

### 5) Dispute

- Dispute is available to both buyer and seller regardless of authorization state.

### 6) Events

- `ReleaseAuthorized { escrow_id, authorizer }`
- `CoSignatureAdded { escrow_id, cosigner }`
- `DualCustodyReleaseCompleted { escrow_id }`
- `ReleaseAuthorizationRevoked { escrow_id, revoker }`

## Current Repository Status (Checked)

### `contracts/ahjoor-escrow`

- Located these source files:
  - `contracts/ahjoor-escrow/src/lib.rs`
  - `contracts/ahjoor-escrow/src/events.rs`
  - multiple existing test modules under `contracts/ahjoor-escrow/src/`
- Confirmed the dual-custody API surface you requested is **not present** in the escrow contract code:
  - no `authorize_release`
  - no `cosign_release`
  - no `revoke_release_authorization`
  - no `update_cosigner`
  - no dual-custody status variants/events matching the spec

### Tooling Limitation Affecting Safe Integration

- The `search_files` tool fails in this environment because **`ripgrep` is missing**.
- Without repository-wide search, it is not possible to reliably locate existing “release authorization” logic to integrate safely.

## Constraints Imposed For This Attempt

- Do **not** modify existing code.
- Do **not** run tests.
- Do **not** run build or eslint.

Given:

1. the dual-custody functions/events do not exist yet, and
2. repository-wide search is unavailable,

…it is not possible to implement the feature under the “no code changes / no risk of bugs” constraints.

## What Would Be Needed To Implement (Next Steps)

1. Enable repository search (install/enable `ripgrep`), or provide exact file/section targets for edits.
2. Implement the following in `contracts/ahjoor-escrow/src/lib.rs`:
   - storage fields for `co_signer`
   - dual-custody state machine (statuses)
   - `authorize_release`, `cosign_release`, `revoke_release_authorization`, `update_cosigner`
   - ensure dispute logic remains accessible
   - ensure release executes atomically when both authorizations are present
3. Add events in `contracts/ahjoor-escrow/src/events.rs`.
4. Add tests covering:
   - full dual-auth flow
   - revocation before second sig
   - co-signer update
   - single-auth-only does not release
   - dispute during auth state

## Implementation Safety Checklist

- Ensure no change breaks existing release paths (buyer-only, arbiter release, milestone/oracle/cooling-off flows).
- Ensure status transitions are mutually consistent with existing escrow statuses.
- Ensure revocation does not allow release after both signatures.
- Ensure update_cosigner is blocked once any release authorization starts.
- Ensure event emission matches the spec.
