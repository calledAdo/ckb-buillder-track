# Lean Oracle

`lean_oracle` is a CKB-based oracle project designed to bring externally signed market data on chain in a way that is transparent, verifiable, and easy for other protocols to consume.

The project focuses on one clear responsibility:

- accept a market-data update fetched from Hermes
- verify that the update is genuinely backed by the expected upstream signer set
- verify that the update belongs to the correct market feed
- store the latest authenticated value in an oracle cell

Rather than trying to make the oracle responsible for every downstream policy decision, `lean_oracle` is intentionally narrow. It is built to answer:

- is this price update authentic?
- does it come from the right source?
- is it newer than the currently stored value?

and then leave application-specific risk rules, such as freshness tolerances, to the protocols that consume the oracle.

## Project Vision

The long-term aim of `lean_oracle` is to give CKB applications a reusable oracle layer that behaves like a verifiable data bridge rather than a black box.

That means:

- the signer set is governed on chain
- the trusted source identity is stored on chain
- the update payload is verified on chain
- the resulting oracle state is deterministic and queryable

This makes the project useful as shared infrastructure for:

- lending protocols
- derivatives protocols
- liquidation systems
- synthetic asset systems
- any CKB application that needs authenticated off-chain market data

## Core Design Philosophy

The project is built around separation of concerns.

### 1. Guardian Set as a Governed Trust Root

`lean_oracle` does not hardcode trusted signers inside the oracle update logic. Instead, it keeps the signer set in a dedicated guardian-set cell.

This gives the design two benefits:

- the oracle can verify updates against an on-chain governed root of trust
- signer rotation can happen independently from the oracle cell itself

### 2. Oracle Cell as Authenticated State

Each oracle cell stores the latest accepted authenticated update for one feed.

The oracle cell is meant to be:

- compact
- deterministic
- easy for downstream protocols to read

It is not meant to duplicate the full upstream payload history. It stores only the current accepted state that other contracts need.

### 3. Authentication First, Policy Second

The oracle verifies authenticity and monotonicity.

It does not try to decide every possible downstream risk policy. For example, different protocols may want different freshness thresholds for the same BTC/USD feed. A trading protocol may require a much tighter freshness window than a slower-moving lending system.

So `lean_oracle` is designed to provide:

- authenticated state

while leaving:

- “is this fresh enough for my protocol?”

to the consumer protocol.

## What The Oracle Verifies

At a high level, a successful update means the oracle has confirmed all of the following:

- the payload is structurally valid
- the payload comes from the expected source
- the guardian quorum backing the payload is valid
- the market message is included in the authenticated batch
- the new update is newer than the old oracle state
- the new oracle output cell exactly matches the authenticated market message

That gives downstream protocols confidence that the oracle cell is not just “some stored price,” but the latest price that this oracle instance has accepted from its configured upstream source.

## Why Hermes Matters

Hermes is the off-chain delivery layer that provides the accumulator update blob.

From the perspective of `lean_oracle`, Hermes is the source of transport, not the source of trust.

In other words:

- Hermes delivers the bytes
- the oracle verifies whether those bytes are authentic

So the project does not trust Hermes blindly. The whole point of the on-chain verification flow is to make Hermes a delivery mechanism rather than a trusted actor.

## Why This Project Matters on CKB

CKB does not natively know anything about BTC/USD, ETH/USD, or any other external market.

For a protocol to act on real-world prices, it needs a way to ingest those prices without breaking determinism. `lean_oracle` is an attempt to solve that in a CKB-native way:

- one governed trust root
- one oracle state cell per feed
- one transaction-based update path
- one reusable verification pipeline for downstream protocols

This is especially important because many DeFi systems on CKB will eventually need a common oracle layer rather than each application inventing its own one-off integration.

## Current Scope

The project already includes:

- a shared common crate for protocol parsing and verification logic
- an oracle type script that validates oracle cell updates
- a guardian-set type script that validates guardian-set cells
- host-side tests for the parsing and verification flow
- integration tests using `ckb-testtool`

That means `lean_oracle` is already beyond the idea stage. It now has a functioning verification pipeline and both fixture-based and transaction-level tests.

## Current Architecture

The workspace is organized into four main parts:

- `contracts/common`
  - shared data structures, parsers, proof helpers, and verifier logic
- `contracts/oracle_script`
  - the main oracle cell validator
- `contracts/guardian_set_script`
  - the guardian-set validator
- `tests`
  - unit and integration tests

This split keeps the project maintainable and makes it easier to reason about what belongs to:

- reusable protocol logic
- state-cell validation
- governance/trust-root validation
- tests and developer tooling

## What The Project Is Not Trying To Do

`lean_oracle` is not trying to be:

- a full market-data business
- a replacement for upstream data publishers
- a global risk engine for every protocol on CKB

Instead, it is trying to be:

- a trustworthy CKB verifier and storage layer for externally delivered market data

That distinction matters because the most valuable part of an oracle ecosystem is often not just parser code, but:

- the publisher network
- the data relationships
- the signer coordination
- the distribution infrastructure

`lean_oracle` focuses on the CKB verification side of that problem.

## Intended Consumer Experience

For a protocol that wants to use `lean_oracle`, the intended interaction model is simple:

1. read the oracle cell for the feed it cares about
2. inspect the currently stored authenticated market state
3. decide whether that state is acceptable for its own application rules
4. if needed, update the oracle before using it

This makes the oracle reusable across multiple applications without forcing them all into the same freshness or risk model.

## Current Testing Status

The project has both:

- host-side parsing and verification tests
- transaction-level integration tests

That includes:

- synthetic fixtures for controlled verification
- real-world fixtures using a live guardian set and a real Hermes payload
- `ckb-testtool` transaction simulation against the actual contract binaries

So the project is already in a good place for iterative hardening.

## Next Direction

The natural next step for `lean_oracle` is continued hardening:

- more adversarial integration tests
- stronger guardian-set governance flow
- better deployment tooling
- easier feed initialization and update workflows
- downstream protocol integrations

Over time, the project can become a reusable oracle foundation for a broader CKB DeFi stack.

## Summary

`lean_oracle` is a focused oracle-verification project for CKB.

Its purpose is to:

- take externally delivered market data
- prove that the data is authentic on chain
- store the latest accepted state in a clean oracle cell

By keeping the oracle narrow and verifiable, the project gives downstream CKB protocols a trustworthy base layer for price-aware logic without forcing every consuming application into the same policy model.
