#![no_std]
#![cfg_attr(not(test), no_main)]

#[cfg(test)]
extern crate alloc;
use ckb_std::default_alloc;

#[cfg(not(test))]
ckb_std::entry!(program_entry);

#[cfg(not(test))]
default_alloc!();

use ckb_std::{
    ckb_constants::Source,
    ckb_types::prelude::*,
    error::SysError,
    high_level::{load_cell_lock, load_cell_type, load_script},
};
use lenderr_common::errors::{ERROR_ENCODING, ERROR_POOL_DATA_MALFORMED};

pub fn program_entry() -> i8 {
    match verify() {
        Ok(_) => 0,
        Err(err) => err,
    }
}

fn verify() -> Result<(), i8> {
    // Load this lock script so we can compute our own hash for output matching.
    let this_lock = load_script().map_err(|_| ERROR_ENCODING)?;
    let this_lock_hash: [u8; 32] = this_lock.calc_script_hash().unpack();

    // args = 32-byte hash of pool_script (the type script for the pool cell).
    // We use this to find the pool cell by its type hash.
    let args: alloc::vec::Vec<u8> = this_lock.args().unpack();
    if args.len() != 32 {
        return Err(ERROR_ENCODING);
    }
    let pool_type_hash: [u8; 32] = args.as_slice().try_into().unwrap();

    if !pool_cell_present(Source::Input, &pool_type_hash) {
        return Err(ERROR_POOL_DATA_MALFORMED);
    }

    // ── Output pool cell must carry the same lock (no lock hijacking) ────────
    // Find the output pool cell's index then load its lock hash.
    let output_idx =
        find_cell_by_type(Source::Output, &pool_type_hash).ok_or(ERROR_POOL_DATA_MALFORMED)?;

    let output_lock =
        load_cell_lock(output_idx, Source::Output).map_err(|_| ERROR_POOL_DATA_MALFORMED)?;
    let output_lock_hash: [u8; 32] = output_lock.calc_script_hash().unpack();

    if output_lock_hash != this_lock_hash {
        // Someone tried to replace pool_lock with a different lock on the output.
        return Err(ERROR_POOL_DATA_MALFORMED);
    }

    // pool_script (type) takes care of validating the accounting.
    // pool_lock's job is purely structural: singleton + continuity.
    Ok(())
}

/// Count how many cells in `source` have type script hash == `target`.
fn pool_cell_present(source: Source, target: &[u8; 32]) -> bool {
    find_cell_by_type(source, target).is_some()
}

/// Return the index of the first cell in `source` whose type hash == `target`.
fn find_cell_by_type(source: Source, target: &[u8; 32]) -> Option<usize> {
    let mut i = 0usize;
    loop {
        match load_cell_type(i, source) {
            Ok(Some(ts)) => {
                let hash: [u8; 32] = ts.calc_script_hash().unpack();
                if &hash == target {
                    return Some(i);
                }
            }
            Ok(None) => {}
            Err(SysError::IndexOutOfBound) => break,
            Err(_) => {}
        }
        i += 1;
    }
    None
}
