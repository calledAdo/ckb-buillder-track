# Payment Channel Type Script

`payment-channel-type` is the **channel state machine**.

It is meant to be placed on a small state cell that uses `always_success` as
its lock, so the state transition rules live entirely in this type script.

The companion UDT escrow cell is protected by `cell-owned-lock`.

---

## Why this split exists

The old single-cell design put both channel identity and mutable dispute state
inside the escrow cell lock args.

That made `channel_id` hard to query directly because the lock args also
contained changing state.

This new design separates concerns:

- the state cell is easy to query directly by `channel_id`
- the escrow cell keeps the standard UDT type unchanged
- the state script owns the state machine and final payout checks
- the escrow lock only owns the money/state linkage

---

## Type args

Length: `32` bytes

| Bytes | Field | Meaning |
|---|---|---|
| `[0..32)` | `channel_id` | unique channel identifier |

On creation, `channel_id` must equal:

- `blake2b("ckb-default-hash", first_input_tx_hash || first_input_index_LE4 || output_index_LE4)`

where:

- `first_input_tx_hash || first_input_index_LE4` is the previous outpoint of
  the transaction's first input cell
- `output_index` is this state cell's actual output index in the transaction

---

## State cell data layout

Length: `160` bytes

| Bytes | Field | Meaning |
|---|---|---|
| `[0..20)` | `seller_blake160` | seller identity |
| `[20..40)` | `buyer_blake160` | buyer identity |
| `[40..72)` | `payout_lock_code_hash` | code hash used for seller/buyer payout outputs |
| `[72..104)` | `escrow_lock_code_hash` | code hash used by linked escrow UDT cells |
| `[104..136)` | `udt_type_hash` | exact UDT type hash used by linked escrow cells |
| `[136]` | `dispute_started` | `0 = open`, `1 = disputed` |
| `[137..144)` | `reserved` | must be zero |
| `[144..160)` | `seller_claim_udt` | `u128` cumulative seller claim |

The first 136 bytes are channel identity plus escrow-filter metadata.
The last 24 bytes are mutable dispute state.

---

## Witness modes

The type script reads `WitnessArgs.lock` from the first group input.

There are three valid layouts:

### 1. Cooperative close

Length: `130` bytes

- `buyer_sig(65) || seller_sig(65)`

Both signatures are over:

- `blake2b("ckb-default-hash", seller_udt(16) || buyer_udt(16) || channel_id(32))`

This is only valid while:

- `dispute_started == 0`

### 2. Start dispute / challenge

Length: `65` bytes

- `buyer_sig(65)`

The buyer signature is over:

- `blake2b("ckb-default-hash", seller_claim(16) || channel_id(32))`

The `seller_claim` itself comes from the output state cell data, not from the
witness.

### 3. Post-dispute close

Length: `0`

This is only valid when:

- the state-cell input uses a relative timestamp `since` of at least 48 hours

---

## State transitions

### 1. Creation

Allowed shape:

- `GroupInput = 0`
- `GroupOutput = 1`

Rules:

- output data must be exactly `160` bytes
- output type args must match the derived `channel_id`
- `dispute_started == 0`
- `seller_claim_udt == 0`

This creates an open channel state.

### 2. Start dispute / challenge

Allowed shape:

- `GroupInput = 1`
- `GroupOutput = 1`

Rules:

- immutable identity fields must not change
- witness must be the 65-byte dispute format
- no linked escrow cells may be included in this transaction
- buyer signature must be valid for the new claim
- `new_seller_claim > current_seller_claim`
- output data must equal:
  - same identities
  - if starting dispute: `dispute_started = 1`
  - if challenging: `dispute_started` stays `1`
  - updated `seller_claim_udt`

If the channel is already disputed, a challenge transaction is valid only when
the buyer presents a strictly higher seller claim.

### 3. Destruction

Allowed shape:

- `GroupInput = 1`
- `GroupOutput = 0`

Two destruction paths exist.

#### Cooperative close

Valid only when:

- `dispute_started == 0`
- witness is the 130-byte cooperative-close format
- both buyer and seller signatures validate against the actual payout outputs
- seller and buyer payout outputs sum to the total amount derived from linked escrow inputs
- no linked escrow outputs remain live after the close transaction

#### Post-dispute close

Valid only when:

- `dispute_started == 1`
- witness is empty
- the state-cell input uses a relative timestamp `since` of at least 48 hours
- seller output equals stored `seller_claim_udt`
- seller and buyer payout outputs sum to the total amount derived from linked escrow inputs
- no linked escrow outputs remain live after the close transaction

---

## Relationship with the escrow lock

This type script enforces the channel logic and the final payout constraints.

In particular:

- it decides whether the state transition is valid
- it derives linked escrow totals by filtering on `escrow_lock_code_hash`, state-cell type hash, and `udt_type_hash`
- it validates the actual seller/buyer payout amounts
- it requires the payout tx to use a 48-hour relative timestamp `since` on the
  state-cell input
- `cell-owned-lock` only ensures escrow cells stay linked to the correct
  state cell

That split is intentional:

- state cell = queryable protocol state
- escrow cell = actual funds

Both must succeed in the same transaction.

---

## Security properties

This script guarantees:

- channel identity is immutable once opened
- the buyer must authorize every dispute claim pushed on-chain
- disputed claims can only move monotonically upward
- cooperative close requires both parties to sign the actual payout split
- unilateral close is delayed by a consensus-enforced 48-hour relative `since`
