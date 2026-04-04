# Cell-Owned Lock

`cell-owned-lock` is a minimal reusable linkage lock.

In this project it is used on escrow-style UDT cells.

The payment channel is split into two scripts:

- `payment-channel-type`
  - lives on a tiny state cell
  - uses `channel_id` as type args
  - owns signatures, dispute progression, and time-gated closure
- `cell-owned-lock`
  - lives on the UDT escrow cell
  - couples the escrow cell to the matching state cell

This split keeps the token type unchanged while making `channel_id` easy to
query on-chain through the state cell.

---

## Why this lock exists

The escrow cell still holds the actual UDT balance.

What we need from the lock is:

- if an escrow cell is spent, the linked cell must also be in the transaction inputs
- multiple escrow UDT cells can point at the same state cell

This lock deliberately does **not** verify:

- signatures
- dispute timestamps
- payout amount correctness

Those are validated by the state type script.

---

## Lock args

Length: `32` bytes

| Bytes | Field | Meaning |
|---|---|---|
| `[0..32)` | `linked_cell_type_hash` | exact type script hash of the linked cell |

Using the full linked-cell type hash means the lock can find the exact linked
cell by equality, instead of scanning and decoding many channel ids.

---

## Companion state requirement

Whenever an escrow input is spent, the escrow lock expects a matching linked
cell in the transaction inputs identified by `linked_cell_type_hash`.

If that input-side linkage is missing, the escrow spend fails.

---

## Runtime model

CKB lock scripts execute for **inputs being consumed**, not for newly created
outputs.

That means this lock does not run at channel-open or funding-output creation
time. It only runs later, when existing escrow cells are being spent.

When it runs, it enforces only one thing:

- a matching linked-cell type hash must exist in the transaction inputs

There is intentionally no escrow 1-to-1 update path.

During dispute updates, only the state cell is consumed and recreated. The
escrow cell stays untouched until the final close transaction.

---

## Security properties

This lock provides these guarantees:

- escrow/state coupling: the money cell cannot move independently of its state
  cell
- frozen escrow updates: dispute updates cannot silently move funds because the
  escrow cell is not part of those transactions at all
- minimality: the escrow lock does not duplicate state-machine logic that
  belongs in the state type script

Together with the state script, this gives a full two-cell payment channel.
