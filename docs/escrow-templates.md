# Escrow Templates

Escrow templates let a creator pre-configure common escrow parameters (arbiter, token,
deadline duration, and renewal/condition policy) so that other parties can spin up
identical escrows without re-specifying every field.

---

## Creating a template

```rust
EscrowTemplateConfig {
    arbiter: arbiter_address,
    token: token_address,
    deadline_duration: 7 * 24 * 3600,      // 1 week in seconds
    renewal_condition_policy: RenewalConditionPolicy::Reset,
}
```

Call `create_escrow_template(creator, config)` — returns a `template_id`.

Anyone may then call `create_escrow_from_template(buyer, seller, template_id, amount)` to
instantiate an escrow that inherits the template's arbiter, token, deadline duration, and
renewal condition policy.

---

## Renewal and ConditionalRelease interaction

An escrow can have two independent features enabled simultaneously:

| Feature | Where configured |
|---------|-----------------|
| **AutoRenewConfig** | `EscrowExtensions.auto_renew_max_renewals` / `auto_renew_interval_ledgers` |
| **ConditionalRelease** | `DataKey2::ConditionalReleaseCondition(escrow_id)` (set via `set_conditional_release`) |

When an escrow renews, a new escrow ID is created. The `ConditionalRelease` condition is
stored under the *old* ID and must be explicitly carried — or cleared — for the new term.

### RenewalConditionPolicy

The `renewal_condition_policy` field on `EscrowExtensions` (and `EscrowTemplateConfig`)
controls this behaviour:

| Variant | Effect on renewal |
|---------|-------------------|
| `Reset` *(default)* | The `ConditionalRelease` condition is **not copied** to the renewed escrow. The new term starts with no condition. |
| `CarryOver` | The condition is **copied** to the new escrow ID. Waiver signatures (`BuyerWaiverSigned`, `SellerWaiverSigned`) are **always cleared** — consent given in a prior term cannot short-circuit the new term's condition check. |

> **Important**: Waiver signatures are reset on renewal under *both* policies. Even under
> `CarryOver`, both parties must re-waive (or the oracle must re-confirm) in each new term.

### Why this matters

Without explicit handling a stale `ConditionalRelease` condition — or a partial waiver
signed in a prior term — could have leaked into the renewed escrow's context. The fix
introduced in this release ensures that:

1. Under `Reset` (default): the renewed escrow is unconditional; a condition must be
   re-established via `set_conditional_release` if needed.
2. Under `CarryOver`: the condition specification persists across terms, but all waiver
   state is wiped so the new term is treated as a fresh obligation.

### Events emitted on renewal

| Event | Emitted when |
|-------|-------------|
| `RenewalConditionReset` | A condition existed on the old escrow and policy is `Reset`. |
| `RenewalConditionCarriedOver` | A condition existed on the old escrow and policy is `CarryOver`. |

If no condition was set on the old escrow, neither event is emitted.

---

## Template-level policy

Set `renewal_condition_policy` in `EscrowTemplateConfig` to encode your preferred default
for all escrows spawned from the template:

```rust
// Template for recurring SaaS subscriptions — always reset conditions each period.
EscrowTemplateConfig {
    arbiter: arbiter_address,
    token: usdc_address,
    deadline_duration: 30 * 24 * 3600,   // 30 days
    renewal_condition_policy: RenewalConditionPolicy::Reset,
}

// Template for rolling supply agreements — condition persists, re-validated each renewal.
EscrowTemplateConfig {
    arbiter: arbiter_address,
    token: usdc_address,
    deadline_duration: 30 * 24 * 3600,
    renewal_condition_policy: RenewalConditionPolicy::CarryOver,
}
```

Escrows created from the template inherit the policy; it can be read from
`escrow.extensions.renewal_condition_policy`.

---

## Renewal history

`get_renewal_history(original_escrow_id)` returns the ordered list of all successor escrow
IDs in the chain (e.g. `[1, 2, 3]` after three renewals of escrow `0`).

The chain root is tracked internally via `DataKey2::RenewalChainRoot` so that history always
accumulates on the first escrow, regardless of how many hops deep the renewal is.
