# Payments Authorization and Capture Flow

This document describes the two-step authorization and capture lifecycle used by
the `ahjoor-payments` contract.

The flow is designed for merchants that need to reserve customer funds first and
settle them later. Authorization moves the funds into contract escrow, capture
settles the payment, and a missed capture window lets the authorization expire.

## Status Transition Diagram

```text
Pending -> Authorized -> Captured  (settled)
                     -> Expired   (missed capture window)
                     -> Disputed
```

In contract storage, a successful capture is finalized as
`PaymentStatus::Completed` and emits `PaymentCaptured`. Integrators may display
that terminal settlement step as "Captured" in user-facing flows.

## Lifecycle

### 1. Authorize

Call:

```text
authorize_payment(merchant, customer, token, amount, capture_deadline_ledger)
```

The merchant/payee signs the contract call. The customer/payer must have already
approved the payment contract to transfer the authorized amount of `token`.

When authorization succeeds:

- `amount` is transferred from the customer into the payment contract.
- A new `Payment` is stored with `status = PaymentStatus::Authorized`.
- `Payment.capture_deadline` is set to `capture_deadline_ledger`.
- `PaymentAuthorized` is emitted.

`capture_deadline_ledger` must be greater than the current ledger sequence. If
it is not in the future, authorization fails.

### 2. Capture

Call:

```text
capture_payment(merchant, payment_id)
```

The merchant/payee signs the capture call. Capture is only valid while the
payment is still `Authorized` and the current ledger sequence is less than or
equal to `Payment.capture_deadline`.

When capture succeeds:

- The payment is finalized and funds settle to the merchant after contract fee
  logic.
- The stored payment reaches the settled terminal state
  `PaymentStatus::Completed`.
- `PaymentCaptured` is emitted with the captured amount.

If capture is attempted after the capture deadline, the contract raises
`CapturePastDeadline`.

### 3. Missed Capture

Call:

```text
expire_payment(payment_id)
```

If an authorized payment is not captured before its capture window closes, it
can be expired. This function is callable by anyone once the current ledger
sequence is greater than `Payment.capture_deadline`.

When expiry succeeds:

- The escrowed amount is transferred back to the customer/payer.
- The stored payment moves to `PaymentStatus::Expired`.
- `PaymentExpired` and `PaymentStatusChanged` are emitted.

### 4. Dispute During Authorization

Call:

```text
dispute_payment(customer, payment_id, reason)
```

An authorized payment can be disputed before it is captured or expired. The
current contract entrypoint requires the customer/payer to sign the dispute
call.

When dispute succeeds:

- The payment moves from `PaymentStatus::Authorized` to
  `PaymentStatus::Disputed`.
- A temporary dispute record is stored with the dispute reason.
- `PaymentDisputed` and `PaymentStatusChanged` are emitted.

Disputed payments leave the normal capture path and must be resolved through the
contract's dispute resolution functions.

## Payment Struct Field

`Payment.capture_deadline` stores the ledger sequence after which an authorized
payment can no longer be captured.

- `0` means the payment is not using the authorization/capture path.
- A non-zero value is set by `authorize_payment`.
- `capture_payment` rejects captures when the current ledger sequence is greater
  than this value.
- `expire_payment` uses this value to decide when an authorized payment can be
  expired.

## Events

### PaymentAuthorized

Emitted when funds are reserved in escrow by `authorize_payment`.

Fields:

| Field | Meaning |
| --- | --- |
| `payment_id` | ID of the newly authorized payment. |
| `customer` | Customer/payer whose funds were moved into escrow. |
| `merchant` | Merchant/payee that can later capture the payment. |
| `amount` | Authorized token amount. |
| `capture_deadline_ledger` | Ledger sequence by which capture must happen. |

### PaymentCaptured

Emitted when an authorized payment is successfully captured and settled.

Fields:

| Field | Meaning |
| --- | --- |
| `payment_id` | ID of the captured payment. |
| `amount` | Gross authorized amount captured before final settlement accounting. |

## Integration Notes

- Ask the customer to approve the payment contract for the token amount before
  calling `authorize_payment`.
- Surface the capture deadline in the UI as a ledger-based countdown, not a wall
  clock timestamp.
- Disable capture actions once the current ledger sequence is greater than
  `Payment.capture_deadline`.
- Offer an expiry action after the deadline so users can release missed
  authorizations back to the payer.
- Treat `PaymentCaptured` plus `PaymentStatus::Completed` as the settled state.
