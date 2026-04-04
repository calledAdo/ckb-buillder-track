//! This file contains the main oracle type script.
//!
//! It is the heart of the `lean_oracle` project. Its update path performs the
//! following sequence:
//! 1. load and decode old/new oracle cells
//! 2. enforce immutable configuration fields
//! 3. load the raw accumulator witness
//! 4. parse the Pyth accumulator update
//! 5. parse the embedded Wormhole VAA
//! 6. load the governed guardian-set cell from `CellDep`
//! 7. verify Wormhole guardian quorum
//! 8. find the authenticated message for this feed
//! 9. require the oracle output cell to match that authenticated message

// Compile without the standard library so the binary can run in CKB-VM.
#![no_std]
// In contract builds, compile as a CKB entrypoint rather than a normal `main`.
#![cfg_attr(not(test), no_main)]

// During host tests we still want access to `alloc`.
#[cfg(test)]
extern crate alloc;

// Pull in the default allocator used by `ckb-std`.
#[cfg(not(test))]
use ckb_std::default_alloc;

// Register the CKB entrypoint symbol.
#[cfg(not(test))]
ckb_std::entry!(program_entry);
// Install the allocator for contract builds.
#[cfg(not(test))]
default_alloc!();

// Import the CKB types and VM loaders we need.
use ckb_std::{
    ckb_constants::Source,
    high_level::{load_cell_data, load_cell_type_hash, load_script, load_witness_args},
};
// Import shared protocol logic from the common crate.
use lean_oracle_common::{
    errors::*,
    guardian_set::GuardianSetData,
    oracle_data::OracleData,
    pyth_accumulator::{ParsedAccumulatorUpdateForFeed, PriceFeedMessage},
    oracle_witness::OracleUpdateWitness,
    wormhole_verify::{verify_guardian_quorum, VerifyError},
};

// CKB entrypoint. Return `0` on success or an `i8` error code on failure.
pub fn program_entry() -> i8 {
    // Load the currently executing script so we can inspect its args.
    let script = match load_script() {
        Ok(s) => s,
        Err(_) => return ERROR_SYSCALL,
    };

    // The script args are expected to be exactly one 32-byte feed id.
    let args = script.args().raw_data();
    if args.len() != 32 {
        return ERROR_ENCODING;
    }

    // If there is no group input, then we are creating the oracle cell.
    let is_creation = load_cell_data(0, Source::GroupInput).is_err();
    if is_creation {
        return validate_creation(&args);
    }

    // Otherwise we are performing a normal oracle update.
    validate_update(&args)
}

// Validate creation of a new oracle cell.
fn validate_creation(feed_id_args: &[u8]) -> i8 {
    // Load the oracle output cell's data.
    let output = match load_cell_data(0, Source::GroupOutput) {
        Ok(data) => data,
        Err(_) => return ERROR_SYSCALL,
    };

    // Decode the oracle output state.
    let state = match OracleData::from_bytes(&output) {
        Some(state) => state,
        None => return ERROR_ORACLE_DATA_MALFORMED,
    };

    // Require the feed id encoded in state to match the script args.
    if state.feed_id.as_slice() != feed_id_args {
        return ERROR_INVALID_FEED_ID;
    }

    // If the state is structurally valid and the feed id matches, creation is
    // allowed.
    0
}

// Validate an update of an existing oracle cell.
fn validate_update(feed_id_args: &[u8]) -> i8 {
    // Load the old oracle data from the input side.
    let input = match load_cell_data(0, Source::GroupInput) {
        Ok(data) => data,
        Err(_) => return ERROR_SYSCALL,
    };
    // Load the new oracle data from the output side.
    let output = match load_cell_data(0, Source::GroupOutput) {
        Ok(data) => data,
        Err(_) => return ERROR_SYSCALL,
    };

    // Decode the old oracle state.
    let old = match OracleData::from_bytes(&input) {
        Some(state) => state,
        None => return ERROR_ORACLE_DATA_MALFORMED,
    };
    // Decode the new oracle state.
    let new = match OracleData::from_bytes(&output) {
        Some(state) => state,
        None => return ERROR_ORACLE_DATA_MALFORMED,
    };

    // Require the new state to keep the same feed id as the script args.
    if new.feed_id.as_slice() != feed_id_args {
        return ERROR_INVALID_FEED_ID;
    }
    // Require all immutable config fields to stay unchanged.
    if !old.static_fields_unchanged(&new) {
        return ERROR_CONFIG_MUTATED;
    }
    // Require the publish time to move forward strictly.
    if new.publish_time <= old.publish_time {
        return ERROR_TIMESTAMP_NOT_MONOTONIC;
    }
    // Load and decode the oracle-update witness.
    let witness = match load_update_witness() {
        Ok(witness) => witness,
        Err(code) => return code,
    };

    // Parse the raw Pyth accumulator update from the witness.
    let accumulator = match ParsedAccumulatorUpdateForFeed::parse_for_feed(
        &witness.accumulator_update,
        &new.feed_id,
    ) {
        Some(update) => update,
        None => return ERROR_ACCUMULATOR_UPDATE_MALFORMED,
    };
    // Borrow the embedded VAA for the next validation steps.
    let vaa = &accumulator.vaa;

    // Require the VAA guardian-set index to match the oracle cell config.
    if vaa.guardian_set_index != new.guardian_set_index {
        return ERROR_GUARDIAN_SET_INDEX_MISMATCH;
    }
    // Require the VAA emitter chain to match the oracle cell config.
    if vaa.emitter_chain as u32 != new.emitter_chain {
        return ERROR_EMITTER_MISMATCH;
    }
    // Require the VAA emitter address to match the oracle cell config.
    if vaa.emitter_address != new.emitter_address {
        return ERROR_EMITTER_MISMATCH;
    }

    // Load the governed guardian-set cell by the type hash stored in oracle
    // state.
    let guardian_set = match load_guardian_set(&new.guardian_set_type_hash) {
        Ok(state) => state,
        Err(code) => return code,
    };

    // Require the guardian-set cell itself to agree with the oracle state's
    // guardian-set index.
    if guardian_set.set_index != new.guardian_set_index {
        return ERROR_GUARDIAN_SET_INDEX_MISMATCH;
    }
    // Verify the guardian quorum carried by the VAA.
    match verify_guardian_quorum(&vaa, &guardian_set) {
        Ok(()) => {}
        Err(VerifyError::Quorum) => return ERROR_GUARDIAN_QUORUM_NOT_MET,
        Err(VerifyError::Order) => return ERROR_GUARDIAN_SIGNATURE_ORDER,
        Err(VerifyError::Signature) => return ERROR_GUARDIAN_SIGNATURE_INVALID,
    }

    // Require the oracle output cell to match the authenticated message exactly.
    if !price_update_matches_state(&accumulator.message, &new) {
        return ERROR_PRICE_UPDATE_MISMATCH;
    }

    // If every structural, cryptographic, and temporal check passed, the
    // update is valid.
    0
}

// Load the oracle-update witness from the first group input.
fn load_update_witness() -> Result<OracleUpdateWitness, i8> {
    // Load the first group input's witness args.
    let witness_args = load_witness_args(0, Source::GroupInput).map_err(|_| ERROR_SYSCALL)?;
    // Extract the `lock` field where this script stores its witness bytes.
    let lock = witness_args
        .lock()
        .to_opt()
        .ok_or(ERROR_WITNESS_MALFORMED)?
        .raw_data();

    // Decode the witness into the typed wrapper.
    OracleUpdateWitness::from_bytes(lock.as_ref()).ok_or(ERROR_WITNESS_MALFORMED)
}

// Load the governed guardian-set cell from `CellDep` by matching its type hash.
fn load_guardian_set(expected_type_hash: &[u8; 32]) -> Result<GuardianSetData, i8> {
    // Track the unique matching dep-cell index if we find one.
    let mut matched_index: Option<usize> = None;
    // Start scanning dep cells from index zero.
    let mut i = 0usize;
    // Walk all dep cells until the VM reports out-of-bounds.
    loop {
        match load_cell_type_hash(i, Source::CellDep) {
            Ok(Some(type_hash)) => {
                // If the dep cell's type hash matches the oracle state's
                // expected guardian-set hash, record it.
                if type_hash.as_slice() == expected_type_hash {
                    // Reject ambiguity if more than one dep matches.
                    if matched_index.is_some() {
                        return Err(ERROR_GUARDIAN_SET_HASH_MISMATCH);
                    }
                    matched_index = Some(i);
                }
            }
            Ok(None) => {}
            Err(_) => break,
        }
        i += 1;
    }

    // Require exactly one match.
    let index = matched_index.ok_or(ERROR_GUARDIAN_SET_NOT_FOUND)?;
    // Load the matched guardian-set cell's data bytes.
    let data = load_cell_data(index, Source::CellDep).map_err(|_| ERROR_SYSCALL)?;
    // Decode the guardian-set cell.
    GuardianSetData::from_bytes(&data).ok_or(ERROR_GUARDIAN_SET_MALFORMED)
}

// Compare a decoded Pyth message against the oracle output state.
fn price_update_matches_state(update: &PriceFeedMessage, state: &OracleData) -> bool {
    update.feed_id == state.feed_id
        && update.price == state.price
        && update.conf == state.conf
        && update.expo == state.expo
        && update.publish_time == state.publish_time
        && update.prev_publish_time == state.prev_publish_time
        && update.ema_price == state.ema_price
        && update.ema_conf == state.ema_conf
}
