//! This file contains the guardian-set type script entrypoint.
//!
//! Its job is intentionally narrow:
//! - allow creation of well-formed guardian-set cells
//! - allow no-op updates
//! - reject backwards set-index moves
//! - reject unauthorized rotations until explicit governance logic is added

// Compile without the standard library so the binary can run in CKB-VM.
#![no_std]
// When not testing on the host, compile as a CKB entrypoint rather than a
// normal Rust `main`.
#![cfg_attr(not(test), no_main)]

// During host tests we still want access to `alloc`.
#[cfg(test)]
extern crate alloc;

// Pull in the default allocator used by `ckb-std` in contract builds.
#[cfg(not(test))]
use ckb_std::default_alloc;

// Register the CKB entrypoint symbol.
#[cfg(not(test))]
ckb_std::entry!(program_entry);
// Install the default allocator for contract builds.
#[cfg(not(test))]
default_alloc!();

// Import the CKB data source enum and cell-data loader.
use ckb_std::{
    ckb_constants::Source,
    high_level::load_cell_data,
};
// Import shared errors and guardian-set decoding.
use lean_oracle_common::{
    errors::*,
    guardian_set::GuardianSetData,
};

// CKB entrypoint. Return `0` on success or an `i8` error code on failure.
pub fn program_entry() -> i8 {
    // Treat the absence of a group input as a creation transaction.
    let is_creation = load_cell_data(0, Source::GroupInput).is_err();
    // Route creation and update paths separately.
    if is_creation {
        return validate_creation();
    }
    // Otherwise validate an update transition.
    validate_update()
}

// Validate creation of a new guardian-set cell.
fn validate_creation() -> i8 {
    // Load the single guardian-set output cell's data.
    let output = match load_cell_data(0, Source::GroupOutput) {
        Ok(data) => data,
        Err(_) => return ERROR_SYSCALL,
    };

    // Decode the guardian-set state.
    let state = match GuardianSetData::from_bytes(&output) {
        Some(state) => state,
        None => return ERROR_GUARDIAN_SET_MALFORMED,
    };

    // Reject empty guardian lists or zero quorum.
    if state.guardian_addresses.is_empty() || state.quorum == 0 {
        return ERROR_GUARDIAN_SET_MALFORMED;
    }
    // Reject impossible quorums that exceed the number of guardians.
    if state.quorum as usize > state.guardian_addresses.len() {
        return ERROR_GUARDIAN_SET_MALFORMED;
    }

    // If the initial state is well formed, creation is allowed.
    0
}

// Validate an update to an existing guardian-set cell.
fn validate_update() -> i8 {
    // Load the old guardian-set data from the input side.
    let input = match load_cell_data(0, Source::GroupInput) {
        Ok(data) => data,
        Err(_) => return ERROR_SYSCALL,
    };
    // Load the new guardian-set data from the output side.
    let output = match load_cell_data(0, Source::GroupOutput) {
        Ok(data) => data,
        Err(_) => return ERROR_SYSCALL,
    };

    // Decode the old guardian-set state.
    let old = match GuardianSetData::from_bytes(&input) {
        Some(state) => state,
        None => return ERROR_GUARDIAN_SET_MALFORMED,
    };
    // Decode the new guardian-set state.
    let new = match GuardianSetData::from_bytes(&output) {
        Some(state) => state,
        None => return ERROR_GUARDIAN_SET_MALFORMED,
    };

    // Guardian-set indices are allowed to stay the same or move forward, but
    // never backwards.
    if new.set_index < old.set_index {
        return ERROR_GUARDIAN_SET_CONTINUITY;
    }

    // Governance-auth rotation rules will be added here.
    // For now we allow pure no-op updates so transaction assembly is easier to
    // test while still rejecting any real mutation.
    if old.governance_fields_unchanged(&new) {
        return 0;
    }

    // Any actual mutation is currently treated as an unauthorized rotation.
    ERROR_GUARDIAN_SET_ROTATION_UNAUTHORIZED
}
