//! Borrow Intent Lock (`borrow_intent_lock`)
//!
//! A user who wants to borrow creates an Intent Cell:
//!
//!   Intent Cell
//!     lock = borrow_intent_lock { owner_lock_hash, asset_b_type_hash,
//!                                  vault_lock_code_hash, min_loan_b,
//!                                  min_duration, max_r_vault }
//!     type = asset_a (collateral) xUDT
//!     data = <u128 LE — collateral amount>
//!
//! The aggregator may spend this cell **without a signature** if and only if:
//!   1. The transaction outputs asset_b to owner_lock_hash with amount >= min_loan_b.
//!   2. A vault output exists whose:
//!      - collateral_amt  >= total collateral from all matching intent inputs
//!      - (t_exp - t_created) >= min_duration
//!      - r_vault         <= max_r_vault
//!      - vault.borrower_lock_hash == owner_lock_hash
//!
//! pool_script's validate_borrow enforces the pool-state math independently.
//! This lock enforces the borrower's slippage floor on loan amount and rate.
//!
//! ## Cancel path
//! If any transaction input has lock_hash == owner_lock_hash the cell unlocks
//! unconditionally, letting the borrower reclaim collateral with their regular key.
//!
//! ## Args layout (136 bytes)
//!   [0..32]    owner_lock_hash    – 32-byte lock hash used for recipient/vault ownership checks
//!   [32..64]   asset_b_type_hash  – type hash of the loan token (e.g. stablecoin)
//!   [64..96]   vault_lock_code_hash – code hash of vault lock script expected in borrow output
//!   [96..112]  min_loan_b         – u128 LE: minimum loan amount to accept
//!   [112..120] min_duration       – u64 LE:  minimum vault lifetime in seconds
//!   [120..136] max_r_vault        – u128 LE: maximum per-second rate to accept

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
    high_level::{load_cell_data, load_cell_lock, load_cell_lock_hash, load_cell_type_hash, load_script},
    syscalls::SysError,
};

use lenderr_common::errors::*;
use lenderr_common::utils::read_sudt_amount;
use lenderr_common::vault_data::{VaultData, VAULT_ARGS_LEN};

pub fn program_entry() -> i8 {
    let script = match load_script() {
        Ok(s) => s,
        Err(_) => return ERROR_SYSCALL,
    };

    let args: Bytes = script.args().unpack();
    if args.len() < 136 {
        return ERROR_ENCODING;
    }
    if !single_group_input() {
        return ERROR_ENCODING;
    }

    let mut owner_lock_hash = [0u8; 32];
    owner_lock_hash.copy_from_slice(&args[0..32]);

    let mut asset_b_type_hash = [0u8; 32];
    asset_b_type_hash.copy_from_slice(&args[32..64]);

    let mut vault_lock_code_hash = [0u8; 32];
    vault_lock_code_hash.copy_from_slice(&args[64..96]);

    let mut min_loan_bytes = [0u8; 16];
    min_loan_bytes.copy_from_slice(&args[96..112]);
    let min_loan_b = u128::from_le_bytes(min_loan_bytes);

    let mut min_dur_bytes = [0u8; 8];
    min_dur_bytes.copy_from_slice(&args[112..120]);
    let min_duration = u64::from_le_bytes(min_dur_bytes);

    let mut max_rate_bytes = [0u8; 16];
    max_rate_bytes.copy_from_slice(&args[120..136]);
    let max_r_vault = u128::from_le_bytes(max_rate_bytes);

    // ── Cancel path (lock-hash presence auth) ─────────────────────────────────
    if owner_is_present(&owner_lock_hash) {
        return 0;
    }

    // ── Aggregator path ───────────────────────────────────────────────────────

    // With singleton enforced, collateral is exactly the xUDT amount in GroupInput[0].
    let total_collateral = group_input_collateral();
    if total_collateral == 0 {
        return ERROR_ENCODING;
    }

    // Check 1: loan delivered to the borrower.
    if !loan_delivered(&owner_lock_hash, &asset_b_type_hash, min_loan_b) {
        return ERROR_LOAN_NOT_DELIVERED;
    }

    // Check 2: a vault with the borrower's terms exists in outputs.
    if !vault_created(
        &owner_lock_hash,
        &vault_lock_code_hash,
        total_collateral,
        min_duration,
        max_r_vault,
    ) {
        return ERROR_VAULT_NOT_CREATED;
    }

    0
}

/// Borrow intent lock group must contain exactly one input cell.
fn single_group_input() -> bool {
    if load_cell_data(0, Source::GroupInput).is_err() {
        return false;
    }
    load_cell_data(1, Source::GroupInput).is_err()
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

/// Reads xUDT amount from GroupInput[0].
fn group_input_collateral() -> u128 {
    match load_cell_data(0, Source::GroupInput) {
        Ok(data) => read_sudt_amount(&data).unwrap_or(0),
        Err(_) => 0,
    }
}

/// Scans outputs for an asset-b cell delivered to the borrower with amount >= min_loan_b.
fn loan_delivered(
    owner_lock_hash: &[u8; 32],
    asset_b_type_hash: &[u8; 32],
    min_amount: u128,
) -> bool {
    let mut i = 0;
    loop {
        let type_hash = match load_cell_type_hash(i, Source::Output) {
            Ok(Some(h)) => h,
            Ok(None) => {
                i += 1;
                continue;
            }
            Err(SysError::IndexOutOfBound) => return false,
            Err(_) => return false,
        };

        if &type_hash != asset_b_type_hash {
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

        if &lock_hash != owner_lock_hash {
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
        if u128::from_le_bytes(buf) >= min_amount {
            return true;
        }

        i += 1;
    }
}

/// Scans outputs for a vault cell that satisfies the borrower's term constraints.
///
/// Vault cells are identified by their lock script args (>= VAULT_ARGS_LEN bytes)
/// rather than cell data, because loan metadata now lives in the args to preserve
/// xUDT data field compatibility. pool_script enforces correct vault_type independently.
///
/// The vault must:
///   - Have borrower_lock_hash == owner_lock_hash
///   - Have collateral_amt >= minimum_collateral
///   - Have (t_exp - t_created) >= min_duration
///   - Have r_vault <= max_r_vault
fn vault_created(
    owner_lock_hash: &[u8; 32],
    vault_lock_code_hash: &[u8; 32],
    minimum_collateral: u128,
    min_duration: u64,
    max_r_vault: u128,
) -> bool {
    let mut i = 0;
    loop {
        let lock = match load_cell_lock(i, Source::Output) {
            Ok(l) => l,
            Err(SysError::IndexOutOfBound) => break,
            Err(_) => {
                i += 1;
                continue;
            }
        };

        // Enforce vault identity by lock code hash.
        if lock.code_hash().as_slice() != vault_lock_code_hash {
            i += 1;
            continue;
        }

        let lock_args: Bytes = lock.args().unpack();

        // Quick length check before heavier parsing.
        if lock_args.len() < VAULT_ARGS_LEN {
            i += 1;
            continue;
        }

        // Read collateral amount from cell data (standard xUDT 16-byte balance).
        let collateral_amt = match load_cell_data(i, Source::Output) {
            Ok(data) => match read_sudt_amount(&data) {
                Some(amt) => amt,
                None => {
                    i += 1;
                    continue;
                }
            },
            Err(_) => {
                i += 1;
                continue;
            }
        };

        if let Some(vault) = VaultData::from_lock_args(lock_args.as_ref(), collateral_amt) {
            // Borrower identity check.
            if &vault.borrower_lock_hash != owner_lock_hash {
                i += 1;
                continue;
            }

            // Collateral check.
            if vault.collateral_amt < minimum_collateral {
                i += 1;
                continue;
            }

            // Duration check.
            let duration = vault.t_exp.saturating_sub(vault.t_created);
            if duration < min_duration {
                i += 1;
                continue;
            }

            // Rate check.
            if vault.r_vault > max_r_vault {
                i += 1;
                continue;
            }

            return true;
        }

        i += 1;
    }
    false
}

