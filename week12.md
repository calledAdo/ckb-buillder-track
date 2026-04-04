# Builder Track Weekly Report — Week 12

**Name:** Adokiye

## ✅ Completed Tasks

- Designed and built the new **Lean Oracle** workspace as a CKB-native oracle verification project.
- Structured the project into separate crates for shared protocol logic, the oracle type script, the guardian-set type script, and tests.
- Implemented the full oracle-state cell layout for storing one authenticated feed per cell.
- Implemented the guardian-set cell layout for storing the governed signer set and quorum configuration.
- Built the oracle update path to consume raw Hermes accumulator updates rather than relying on pre-decoded helper data.
- Implemented parsing for the outer Pyth accumulator wrapper, the embedded Wormhole VAA, and the authenticated price-feed message format.
- Implemented Wormhole-style guardian quorum verification against an on-chain guardian-set cell loaded through `CellDep`.
- Implemented Pyth-style Merkle proof verification so individual feed messages are proven against the signed batch root.
- Finalized the oracle responsibility model so the oracle enforces **authenticity + source identity + monotonic publish time**, while freshness checks are left to downstream protocols.
- Added both host-side tests and `ckb-testtool` integration tests, including a real BTC/USD update path using a live guardian set and a real Hermes payload.

## 📚 Key Learning Areas

### 1. Oracle Verification on CKB Is a State-Transition Problem

One of the biggest insights this week was that building an oracle on CKB is not just about storing prices. It is about validating a very specific state transition:

- old oracle cell state
- new oracle cell state
- witness carrying an external update
- governed trust root carried in a dep cell

That means the oracle behaves more like a deterministic verifier than a simple storage contract. The cell update is only valid if the payload, signatures, proof path, and resulting state all agree with one another.

### 2. Separation of Trust Root and Oracle State

The project was deliberately split into:

- a **guardian-set cell**
- an **oracle cell**

This was an important architectural decision. Rather than hardcoding the trusted signer set inside the oracle logic, the oracle loads the guardian-set from a separate governed cell. That makes the system cleaner and easier to upgrade:

- oracle cell = latest authenticated state
- guardian-set cell = current trust root

This separation mirrors a common CKB pattern: one cell for live protocol state and another for governed or slowly changing configuration.

### 3. Pyth Data on Other Chains Is Still a Cross-Chain Verification Problem

Although Pyth’s data originates from its own publisher and aggregation network, consuming it on another chain means verifying the cross-chain transport path. That transport path brings in a different security boundary:

- the Wormhole guardian set
- the Wormhole emitter identity
- the signed Merkle root
- the inclusion proof for the exact feed message

This week made it much clearer that a CKB oracle consuming Hermes updates is not merely “reading Pyth data.” It is reproducing a cross-chain verification model in a CKB-native contract context.

### 4. Raw External Payloads Must Be Parsed in Layers

The Hermes update blob is not one flat message. It is layered:

- outer accumulator wrapper
- embedded Wormhole VAA
- Wormhole payload containing a signed Pyth root
- price-feed messages plus proofs

Implementing this made the update path feel much more like protocol archaeology than ordinary contract coding. Each layer had to be parsed carefully before the next one could even be understood.

The project now has explicit parsers for:

- the accumulator envelope
- the VAA envelope
- the signed root payload
- the feed message body

That layered structure was one of the most important conceptual breakthroughs this week.

### 5. Merkle Proofs Are the Bridge Between Batch Signing and Per-Feed State

The guardians do not sign every single BTC/USD or ETH/USD value individually. Instead, they sign a root. The proof system is what lets a single feed update be accepted on chain later.

This made the role of Merkle proofs much clearer:

- signatures authenticate the batch root
- proofs authenticate the individual feed message
- the oracle state stores the one feed message the protocol cares about

Without that proof step, the VAA would only prove that *some* batch existed, not that the output oracle cell matched a real member of that batch.

### 6. Monotonic Publish Time Is the Right Oracle-Level Time Rule

Initially, it was tempting to make the oracle itself enforce freshness. But during implementation and review, it became clear that freshness is application-specific. Different protocols want different tolerances for “acceptable age.”

The cleaner model is:

- oracle enforces `new.publish_time > old.publish_time`
- consumer protocol decides whether the current oracle state is fresh enough for its own use case

That is a better match for pull-based oracle systems because on-chain updates may legitimately skip many intermediate off-chain updates.

### 7. Real Fixture Testing Matters

One of the strongest parts of the work this week was moving from:

- synthetic byte fixtures

to:

- a real guardian set
- a real Hermes BTC/USD payload
- a real `ckb-testtool` transaction

This was extremely valuable because it validated not just the idea of the verifier, but the actual byte-level assumptions the verifier was making. The real payload also forced careful alignment of:

- guardian-set index
- emitter address
- parsed BTC/USD values

This is where the project started to feel less like a prototype parser and more like a real oracle verifier.

## 🛑 Assumptions Corrected

1. **The Oracle Itself Should Enforce Freshness**
   - *Assumption*: Initially assumed the oracle layer should decide whether a price update was stale enough to reject.
   - *Correction*: Freshness is protocol-specific, not oracle-specific. The oracle now enforces authenticity and monotonicity only, while consumer protocols are expected to decide whether the stored `publish_time` is recent enough for their own risk model.

2. **`prev_publish_time` Should Equal the Previously Stored On-Chain Publish Time**
   - *Assumption*: It initially seemed natural to require `new.prev_publish_time == old.publish_time`.
   - *Correction*: In a pull-based oracle, on-chain updates can skip many intermediate off-chain updates. `prev_publish_time` refers to the previous upstream Pyth update, not necessarily the last CKB-stored one. The only essential oracle-level time rule is that `new.publish_time > old.publish_time`.

3. **A Full Batch Parse Was Necessary for Every Oracle Update**
   - *Assumption*: The first implementation parsed and stored all authenticated messages from the accumulator batch before selecting the target feed.
   - *Correction*: For a single-feed oracle cell, this was unnecessary work. The optimized parser now streams through the batch and only fully materializes the one target feed message, while still preserving the critical verification guarantees.

## 🧪 Verification

The Lean Oracle workspace now passes both host-side and transaction-level checks:

```bash
cd lean_oracle
cargo test -p tests --offline
cargo check --workspace --offline
```

This includes:

- parser and verifier unit tests
- real-fixture tests using a live guardian set and a real Hermes payload
- `ckb-testtool` integration tests against the actual `oracle_type` and `guardian_set_type` binaries

This week's work established Lean Oracle as a working CKB oracle verification foundation rather than just a design concept.
