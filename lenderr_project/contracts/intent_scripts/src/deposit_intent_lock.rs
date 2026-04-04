//! Deposit Intent Lock (`deposit_intent_lock`)
//!
//! A user who wants to deposit liquidity creates an Intent Cell:
//!
//!   Intent Cell
//!     lock = deposit_intent_lock { owner_lock_hash, lp_token_type_hash, min_lp_amount }
//!     type = asset_b xUDT
//!     data = <u128 LE — amount of asset_b being deposited>
//!
//! The aggregator may spend this cell **without a signature** if and only if
//! the transaction contains an LP-token output of amount >= min_lp_amount
//! delivered to owner_lock_hash.
//!
//! ## Cancel path
//! If any transaction input has lock_hash == owner_lock_hash the cell unlocks
//! unconditionally, letting the owner reclaim their funds with their regular key.
//!
//! ## Args layout (80 bytes)
//!   [0..32]  owner_lock_hash    – lender's wallet and LP delivery recipient
//!   [32..64] lp_token_type_hash – type hash of the pool's LP xUDT
//!   [64..80] min_lp_amount      – u128 LE: minimum LP tokens to accept

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
    high_level::{load_cell_data, load_cell_lock_hash, load_cell_type_hash, load_script},
    syscalls::SysError,
};

use lenderr_common::errors::*;

pub fn program_entry() -> i8 {
    let script = match load_script() {
        Ok(s) => s,
        Err(_) => return ERROR_SYSCALL,
    };

    let args: Bytes = script.args().unpack();
    if args.len() < 80 {
        return ERROR_ENCODING;
    }

    let mut owner_lock_hash = [0u8; 32];
    owner_lock_hash.copy_from_slice(&args[0..32]);

    let mut lp_token_type_hash = [0u8; 32];
    lp_token_type_hash.copy_from_slice(&args[32..64]);

    let mut min_lp_bytes = [0u8; 16];
    min_lp_bytes.copy_from_slice(&args[64..80]);
    let min_lp_amount = u128::from_le_bytes(min_lp_bytes);

    // ── Cancel path (lock-hash presence auth) ─────────────────────────────────
    if owner_is_present(&owner_lock_hash) {
        return 0;
    }

    // ── Aggregator path ───────────────────────────────────────────────────────
    // Scan outputs for an LP-token cell with amount >= min_lp_amount that belongs
    // to the owner. The aggregator may arrange outputs however it likes as long as
    // this condition is satisfied.
    let mut i = 0;
    loop {
        let type_hash = match load_cell_type_hash(i, Source::Output) {
            Ok(Some(h)) => h,
            Ok(None) => {
                i += 1;
                continue;
            }
            Err(SysError::IndexOutOfBound) => break,
            Err(_) => return ERROR_SYSCALL,
        };

        if type_hash != lp_token_type_hash {
            i += 1;
            continue;
        }

        let lock_hash = match load_cell_lock_hash(i, Source::Output) {
            Ok(h) => h,
            Err(_) => {
                i += 1;
                continue;
            }
        };

        if lock_hash != owner_lock_hash {
            i += 1;
            continue;
        }

        let data = match load_cell_data(i, Source::Output) {
            Ok(d) => d,
            Err(_) => {
                i += 1;
                continue;
            }
        };

        if data.len() < 16 {
            i += 1;
            continue;
        }

        let mut buf = [0u8; 16];
        buf.copy_from_slice(&data[0..16]);
        let lp_amount = u128::from_le_bytes(buf);

        if lp_amount >= min_lp_amount {
            return 0; // LP delivered at or above user's slippage floor.
        }

        i += 1;
    }

    // No qualifying LP output found — reject.
    ERROR_LP_NOT_DELIVERED
}

/// Returns true if any transaction input has lock_hash == owner_lock_hash.
fn owner_is_present(owner_lock_hash: &[u8; 32]) -> bool {
    let mut i = 0;
    loop {
        match load_cell_lock_hash(i, Source::Input) {
            Ok(h) => {
                if &h == owner_lock_hash {
                    return true;
                }
            }
            Err(SysError::IndexOutOfBound) => return false,
            Err(_) => return false,
        }
        i += 1;
    }
}
