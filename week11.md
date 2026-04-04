# Builder Track Weekly Report — Week 11

**Name:** Adokiye

## ✅ Completed Tasks

- Built the new `payment-channel-type` contract in the `contracts/scripts/` workspace as the main state machine for a CKB payment-channel design.
- Built the companion `cell-owned-lock` escrow lock script so the payment-channel architecture remains cleanly split across a state cell and an escrow cell.
- Studied and documented the full `payment-channel-type` state machine: creation, dispute update, cooperative close, and post-dispute close.
- Traced the on-chain channel identity model based on a derived `channel_id` stored in the type script args.
- Analyzed the fixed 160-byte channel state layout and how immutable identity fields are separated from mutable dispute fields.
- Reviewed the witness protocol for the three supported channel actions:
  - cooperative close
  - buyer-authorized dispute / challenge
  - unilateral post-dispute settlement after timeout
- Understood how linked escrow cells are discovered using the escrow lock code hash, the state cell type hash, and the exact UDT type hash.
- Verified how the script enforces final payout conservation so seller and buyer outputs must exactly exhaust the linked escrow balance.

## 📚 Key Learning Areas

### 1. Splitting State from Funds on CKB

The most important design idea in this payment-channel system is the separation between:

- the **state cell**, which holds the channel state and is indexed by `channel_id`
- the **escrow UDT cell**, which holds the actual funds

The `payment-channel-type` script lives on a small state cell, while the escrow cell is protected by `cell-owned-lock`. This split solves a practical indexing problem from the older design: if mutable dispute state is packed directly into escrow lock args, the channel becomes harder to query by a stable identifier.

With the new model:

- the state cell stays queryable by a fixed `channel_id`
- the escrow cell keeps its UDT type unchanged
- the type script owns the protocol rules
- the escrow lock only enforces state-fund linkage

This is a very CKB-native pattern: use one cell for discoverable protocol state and another for asset custody.

### 2. `channel_id` as a Deterministic Type Arg

The type args of `payment-channel-type` are exactly 32 bytes: the `channel_id`.

On creation, the script recomputes this `channel_id` from:

```text
blake2b(first_input_outpoint || output_index)
```

More precisely, it hashes:

- the first input's previous outpoint (`tx_hash || index_LE4`)
- the actual output index of the new state cell

This is an elegant way to create a stable unique identifier without relying on any off-chain registry. It also means the state cell can be looked up directly by its type args, which is useful for off-chain services and channel monitoring.

### 3. Fixed-Size State Layout and Immutability Boundaries

The state data is exactly **160 bytes** long. The first **136 bytes** define identity and escrow-filter metadata:

- seller identity
- buyer identity
- payout lock code hash
- escrow lock code hash
- UDT type hash

The last **24 bytes** define mutable dispute state:

- `dispute_started`
- reserved zero bytes
- `seller_claim_udt`

This separation is important because the script explicitly compares input and output state during updates and rejects any attempt to change the identity region. Only the dispute region is allowed to evolve.

That gives the channel two strong properties:

- the participants and token configuration can never be swapped mid-channel
- dispute progress can move forward without mutating the channel's core identity

### 4. Buyer-Signed Monotonic Dispute Updates

The update path (`GroupInput = 1`, `GroupOutput = 1`) is used only for dispute or challenge transactions.

The witness format is a single 65-byte buyer signature. The claimed cumulative seller payout is not stored in the witness; instead it is read from the **output state cell** and the buyer signature authorizes:

```text
blake2b(seller_claim_udt || channel_id)
```

This creates a clean security model:

- the buyer must authorize every claim that is pushed on chain
- the claim is tied to one exact channel
- the seller claim must be strictly greater than the previous one
- once dispute starts, it can only move forward monotonically

That monotonicity rule is crucial. It prevents replaying an older, lower-value claim after a newer ticket already exists on chain.

### 5. Two Close Paths: Cooperative and Unilateral

The destroy path (`GroupInput = 1`, `GroupOutput = 0`) supports two settlement modes.

**Cooperative close**

- allowed only while `dispute_started == 0`
- requires a 130-byte witness: buyer signature + seller signature
- both parties sign the exact final payout split:

```text
blake2b(seller_udt || buyer_udt || channel_id)
```

This means the signatures are bound to the actual transaction outputs, not just to a vague intention to close the channel.

**Post-dispute close**

- allowed only after `dispute_started == 1`
- requires an empty witness
- requires a relative timestamp `since` on the state-cell input of at least 48 hours
- requires the seller payout to equal the stored `seller_claim_udt`

This is a strong unilateral-settlement design. The buyer can publish newer tickets during the challenge period, but once the timeout passes, settlement becomes objective and script-enforced.

### 6. Escrow Discovery and Payout Conservation

One of the strongest parts of this script is how it tracks escrow funds without putting all logic into the lock itself.

The script scans transaction inputs and outputs and sums linked escrow cells by checking three things together:

- the escrow cell lock `code_hash` matches `escrow_lock_code_hash`
- the escrow lock args equal the full state-cell type hash
- the escrow cell type hash matches the configured `udt_type_hash`

This prevents unrelated cells from being counted accidentally.

During close, the script enforces:

- all linked input escrow value is accounted for
- no linked escrow outputs remain alive
- seller and buyer payout outputs add up exactly to the escrow total

So even though the funds sit in separate cells, the type script still guarantees full accounting at settlement time.

### 7. Minimal Escrow Lock, Strong State Script

The companion `cell-owned-lock` is intentionally minimal. Its only job is to ensure that if an escrow cell is spent, the matching linked state cell is also present in the transaction inputs.

That is a powerful design choice:

- the escrow lock does not try to understand disputes, signatures, or payout math
- the payment-channel type script handles all protocol semantics
- both scripts succeed together in one transaction

This keeps each contract narrow in responsibility, which usually makes CKB scripts easier to reason about, test, and upgrade.

## 🛑 Assumptions Corrected

1. **The Escrow Lock Should Enforce the Entire Channel Protocol**
   - *Assumption*: Since the escrow cell holds the real funds, it seemed natural that the escrow lock should also validate dispute state and payout rules.
   - *Correction*: The cleaner architecture is the opposite: let the escrow lock only enforce linkage to the channel state cell, and let the `payment-channel-type` script own the full state machine. This preserves a standard UDT escrow cell while keeping protocol logic centralized in the type script.

2. **Dispute Updates Should Carry the Claim in the Witness**
   - *Assumption*: A dispute transaction would likely put both the new claim amount and the signature inside the witness.
   - *Correction*: The script instead reads the proposed `seller_claim_udt` from the output state cell and uses the witness only for the buyer signature. This is cleaner because the state transition is represented directly in cell data, while the witness only proves authorization.

3. **A Cooperative Close Signature Only Needs to Say “Close the Channel”**
   - *Assumption*: It initially seemed enough for both parties to sign the `channel_id` or a generic close message.
   - *Correction*: The script signs the exact payout amounts plus the `channel_id`. That is much safer because signatures are bound to the real seller and buyer outputs, preventing ambiguous or reusable close approvals.

## 🧪 Verification

The payment-channel and escrow-lock crates are now present in the local workspace:

```bash
cd contracts
cargo metadata --no-deps
```

The source contracts documented this week are:

- `contracts/scripts/payment-channel-type`
- `contracts/scripts/cell-owned-lock`

This week's documentation reflects the current implementation of the payment-channel design in this repository.
