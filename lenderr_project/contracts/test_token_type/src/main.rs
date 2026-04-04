//! test_token_type — minimal sUDT for integration tests.
//!
//! Logic:
//! - Owner mode  : args[0..32] == some input cell's lock_hash → mint/burn freely.
//! - User mode   : sum(GroupInput) >= sum(GroupOutput) i.e. no inflation.
//!
//! Wire format: cell data = u128 LE amount (first 16 bytes).

#![no_std]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use ckb_std::default_alloc;

#[cfg(not(test))]
ckb_std::entry!(program_entry);
#[cfg(not(test))]
default_alloc!();

use ckb_std::{
    ckb_constants::Source,
    ckb_types::{bytes::Bytes, prelude::*},
    high_level::{load_cell_data, load_cell_lock_hash, load_script},
    syscalls::SysError,
};

const ERROR_ENCODING: i8 = -1;
const ERROR_SYSCALL: i8 = -2;
const ERROR_INSUFFICIENT_BALANCE: i8 = 52;

pub fn program_entry() -> i8 {
    // Load this script to read args.
    let script = match load_script() {
        Ok(s) => s,
        Err(_) => return ERROR_SYSCALL,
    };
    let args: Bytes = script.args().unpack();

    // args[0..32] = owner lock hash — must be at least 32 bytes for basic sUDT.
    if args.len() < 32 {
        return ERROR_ENCODING;
    }
    let mut owner_lock_hash = [0u8; 32];
    owner_lock_hash.copy_from_slice(&args[0..32]);

    // Owner mode: if the owner lock hash is present in any input cell's lock,
    // we allow any operations (minting, burning, transfer).
    if owner_is_present(&owner_lock_hash) {
        return 0;
    }

    // User mode: enforce conservation (total input tokens >= total output tokens).
    // This allows transfers and burning, but strictly forbids net minting.
    let input_total = match sum_group_amount(Source::GroupInput) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let output_total = match sum_group_amount(Source::GroupOutput) {
        Ok(v) => v,
        Err(code) => return code,
    };

    if input_total < output_total {
        return ERROR_INSUFFICIENT_BALANCE;
    }

    0
}

/// Scan all inputs and return true if any has lock_hash == owner_lock_hash.
fn owner_is_present(owner_lock_hash: &[u8; 32]) -> bool {
    let mut i = 0usize;
    loop {
        match load_cell_lock_hash(i, Source::Input) {
            Ok(lock_hash) => {
                if &lock_hash == owner_lock_hash {
                    return true;
                }
            }
            Err(SysError::IndexOutOfBound) => return false,
            Err(_) => return false,
        }
        i += 1;
    }
}

/// Sum the sUDT u128 LE amounts for all cells in the given source group.
fn sum_group_amount(source: Source) -> Result<u128, i8> {
    let mut i = 0usize;
    let mut total = 0u128;
    loop {
        match load_cell_data(i, source) {
            Ok(data) => {
                if data.len() < 16 {
                    return Err(ERROR_ENCODING);
                }
                let mut buf = [0u8; 16];
                buf.copy_from_slice(&data[0..16]);
                let amount = u128::from_le_bytes(buf);

                // Check for overflow. Real sUDT should not allow overflow even in sum.
                total = match total.checked_add(amount) {
                    Some(v) => v,
                    None => return Err(ERROR_SYSCALL), // Reuse syscall error for internal VM issues like overflow
                };
            }
            Err(SysError::IndexOutOfBound) => return Ok(total),
            Err(_) => return Err(ERROR_SYSCALL),
        }
        i += 1;
    }
}
