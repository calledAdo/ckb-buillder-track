//! This file contains the canonical error-code table for the lean_oracle
//! project.
//!
//! Every script in this workspace returns compact `i8` status codes to CKB.
//! Centralizing them here gives us:
//! - stable failure semantics
//! - easier debugging in tests
//! - one place to evolve the protocol's error vocabulary

// Generic encoding/parsing failure. This is used when transaction bytes,
// witness bytes, or script args cannot be interpreted safely.
pub const ERROR_ENCODING: i8 = -1;
// Generic syscall failure. This covers failures while loading cells, headers,
// witnesses, or scripts from the CKB VM.
pub const ERROR_SYSCALL: i8 = -2;
// Generic authorization failure reserved for governance-style checks.
pub const ERROR_UNAUTHORIZED: i8 = -3;

// The oracle cell's raw data did not match the expected binary layout.
pub const ERROR_ORACLE_DATA_MALFORMED: i8 = 10;
// The guardian-set cell's raw data did not match the expected binary layout.
pub const ERROR_GUARDIAN_SET_MALFORMED: i8 = 11;
// The oracle script args did not match the feed identifier encoded in state.
pub const ERROR_INVALID_FEED_ID: i8 = 12;
// The new publish time did not strictly advance relative to the previous one.
pub const ERROR_TIMESTAMP_NOT_MONOTONIC: i8 = 13;
// Static oracle configuration fields were changed during a normal update.
pub const ERROR_CONFIG_MUTATED: i8 = 14;
// The oracle witness did not match the expected witness layout.
pub const ERROR_WITNESS_MALFORMED: i8 = 15;
// The embedded Wormhole VAA failed basic structural parsing.
pub const ERROR_VAA_MALFORMED: i8 = 16;
// The VAA references a guardian-set index that does not match the oracle cell.
pub const ERROR_GUARDIAN_SET_INDEX_MISMATCH: i8 = 17;
// The VAA emitter chain or emitter address does not match the oracle config.
pub const ERROR_EMITTER_MISMATCH: i8 = 18;
// The required guardian-set dep cell could not be found in the transaction.
pub const ERROR_GUARDIAN_SET_NOT_FOUND: i8 = 19;
// More than one guardian-set dep matched, or the located one was ambiguous.
pub const ERROR_GUARDIAN_SET_HASH_MISMATCH: i8 = 20;
// The VAA did not contain enough guardian signatures to satisfy quorum.
pub const ERROR_GUARDIAN_QUORUM_NOT_MET: i8 = 21;
// A guardian signature failed recovery or did not map to the governed set.
pub const ERROR_GUARDIAN_SIGNATURE_INVALID: i8 = 22;
// Guardian signatures were not ordered strictly by guardian index.
pub const ERROR_GUARDIAN_SIGNATURE_ORDER: i8 = 23;
// The authenticated price-feed message could not be parsed.
pub const ERROR_PRICE_UPDATE_MALFORMED: i8 = 24;
// The authenticated price-feed message did not match the oracle output cell.
pub const ERROR_PRICE_UPDATE_MISMATCH: i8 = 25;
// The raw Pyth accumulator update could not be parsed or verified.
pub const ERROR_ACCUMULATOR_UPDATE_MALFORMED: i8 = 26;

// The new guardian-set index moved backwards instead of staying monotonic.
pub const ERROR_GUARDIAN_SET_CONTINUITY: i8 = 30;
// A guardian-set rotation attempted to change governed fields without an
// explicit governance path.
pub const ERROR_GUARDIAN_SET_ROTATION_UNAUTHORIZED: i8 = 31;
