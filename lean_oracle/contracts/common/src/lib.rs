//! This file is the root module map for the shared `lean-oracle-common` crate.
//!
//! It does not hold business logic itself. Instead, it centralizes the list of
//! submodules that make up the reusable oracle verification library.
//!
//! In practice, this crate is where we keep the protocol knowledge that should
//! be shared between:
//! - the oracle state validator
//! - the guardian set validator
//! - host-side tests
//! - any future off-chain tooling that wants to mirror the same parsing rules

// Compile this crate without the Rust standard library so it remains usable
// from CKB contract crates.
#![no_std]
// Pull in `alloc` because several modules need heap-backed containers such as
// `Vec`, even though we still avoid the full standard library.
extern crate alloc;

// Shared numeric error codes returned by the scripts.
pub mod errors;
// Guardian-set cell parsing and encoding.
pub mod guardian_set;
// Generic and Pyth-specific Merkle helpers.
pub mod merkle;
// Oracle cell parsing and encoding.
pub mod oracle_data;
// Pyth accumulator parsing and authenticated price message extraction.
pub mod pyth_accumulator;
// Witness encoding used by oracle update transactions.
pub mod oracle_witness;
// Wormhole VAA parsing.
pub mod wormhole_vaa;
// Wormhole guardian signature verification.
pub mod wormhole_verify;
