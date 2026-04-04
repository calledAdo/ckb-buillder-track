//! Withdraw Intent Lock (`withdraw_intent_lock`)
//!
//! A user who wants to redeem LP tokens creates an Intent Cell:
//!
//!   Intent Cell
//!     lock = withdraw_intent_lock { owner_lock_hash, asset_b_type_hash, min_asset_b_amount }
//!     type = lp_token xUDT
//!     data = <u128 LE — LP tokens being burned>
//!
//! The aggregator may spend this cell **without a signature** if and only if
//! the transaction contains an asset-b output of amount >= min_asset_b_amount
//! delivered to owner_lock_hash.
//!
//! pool_script's validate_withdraw handles all the exchange-rate math and
//! enforces the correct LP burn accounting. This lock only enforces the
//! user's slippage floor — the minimum underlying assets they're willing to accept.
//!
//! ## Cancel path
//! If any transaction input has lock_hash == owner_lock_hash the cell unlocks
//! unconditionally, letting the owner reclaim their LP tokens with their regular key.
//!
//! ## Args layout (80 bytes)
//!   [0..32]  owner_lock_hash    – LP-holder's wallet and payout recipient
//!   [32..64] asset_b_type_hash  – type hash of the underlying token to receive
//!   [64..80] min_asset_b_amount – u128 LE: minimum underlying tokens to accept

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

    let mut asset_b_type_hash = [0u8; 32];
    asset_b_type_hash.copy_from_slice(&args[32..64]);

    let mut min_bytes = [0u8; 16];
    min_bytes.copy_from_slice(&args[64..80]);
    let min_asset_b_amount = u128::from_le_bytes(min_bytes);

    // ── Cancel path (lock-hash presence auth) ─────────────────────────────────
    if owner_is_present(&owner_lock_hash) {
        return 0;
    }

    // ── Aggregator path ───────────────────────────────────────────────────────
    // Scan outputs for an asset-b cell with amount >= min_asset_b_amount that
    // belongs to the owner. The aggregator calculates the exact payout amount from
    // the exchange rate; we only enforce the user's minimum floor.
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

        if type_hash != asset_b_type_hash {
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
        let asset_b_amount = u128::from_le_bytes(buf);

        if asset_b_amount >= min_asset_b_amount {
            return 0; // Underlying asset delivered at or above user's slippage floor.
        }

        i += 1;
    }

    // No qualifying asset-b output found — reject.
    ERROR_ASSET_NOT_DELIVERED
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
