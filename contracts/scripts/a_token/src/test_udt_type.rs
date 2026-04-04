#![no_std]
#![cfg_attr(not(test), no_main)]

#[cfg(test)]
extern crate alloc;

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
    let script = match load_script() {
        Ok(s) => s,
        Err(_) => return ERROR_SYSCALL,
    };
    let args: Bytes = script.args().unpack();
    if args.len() < 32 {
        return ERROR_ENCODING;
    }

    let mut owner_lock_hash = [0u8; 32];
    owner_lock_hash.copy_from_slice(&args[0..32]);

    // Owner mode: if owner lock hash appears in tx inputs, mint/burn is allowed.
    if owner_is_present(&owner_lock_hash) {
        return 0;
    }

    // User mode: enforce conservation, allowing burn (out <= in).
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
                total = total.saturating_add(u128::from_le_bytes(buf));
            }
            Err(SysError::IndexOutOfBound) => return Ok(total),
            Err(_) => return Err(ERROR_SYSCALL),
        }
        i += 1;
    }
}

