//! # Cell-Owned Lock
//!
//! This lock protects a UDT escrow cell.
//!
//!
//! This lock enforces only linkage:
//! - if an escrow cell is spent, the matching linked cell must also be present
//!   in the transaction inputs
//!
//! ## Lock args
//!
//! The lock args are exactly one 32-byte `linked_cell_type_hash`.
//!
//! That hash is the full type-script hash of the linked cell this escrow must
//! travel with.
//!
//! Because of that, the lock has just one check:
//! - a matching linked-cell type hash must exist in the transaction inputs

#![cfg_attr(not(any(feature = "library", test)), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(any(feature = "library", test))]
extern crate alloc;

mod error;

use core::result::Result::{self, Err, Ok};

use ckb_std::{
    ckb_constants::Source,
    error::SysError,
    high_level::{load_cell_type_hash, load_script},
};

// Re-export the escrow lock's local error enum into this module.
use error::Error;

// Lock args carry one exact linked-cell type-script hash.
const LINKED_CELL_TYPE_HASH_LEN: usize = 32;

#[cfg(not(any(feature = "library", test)))]
ckb_std::entry!(program_entry);
#[cfg(not(any(feature = "library", test)))]
ckb_std::default_alloc!(16384, 1258306, 64);

pub fn program_entry() -> i8 {
    // Convert Rust `Result` into the raw CKB i8 exit code.
    match run() {
        Ok(()) => 0,
        Err(e) => e as i8,
    }
}

fn run() -> Result<(), Error> {
    // Load the currently executing escrow lock so we can inspect its args.
    let script = load_script()?;
    // The args are the exact linked-cell type-script hash.
    let linked_cell_type_hash = script.args().raw_data();
    // Reject malformed lock args immediately.
    if linked_cell_type_hash.len() != LINKED_CELL_TYPE_HASH_LEN {
        return Err(Error::InvalidArgsLength);
    }

    // Spending escrow requires the linked cell to be present in the transaction inputs.
    if find_type_hash(&linked_cell_type_hash, Source::Input)?.is_none() {
        return Err(Error::MissingCompanionCell);
    }

    return Ok(());
}

fn find_type_hash(target: &[u8], source: Source) -> Result<Option<usize>, Error> {
    // Scan cells on one side of the transaction and return the first matching
    // type-script hash. Using this lock assumes the linked type hash is already
    // globally unique by construction.
    let mut i = 0usize;

    loop {
        match load_cell_type_hash(i, source) {
            Ok(Some(type_hash)) if type_hash.as_slice() == target => return Ok(Some(i)),
            Ok(_) => {}
            Err(SysError::IndexOutOfBound) => return Ok(None),
            Err(e) => return Err(e.into()),
        }

        i += 1;
    }
}
