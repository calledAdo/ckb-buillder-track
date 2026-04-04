//! LendNerv Vault Lock Script (`vault_lock.rs`)
//!
//! Technical Overview:
//! This is the **Lock Script** physically attached to every active `Loan_Vault_Cell`.
//! In the Nervos model, lock scripts answer one question: "Who is authorised to consume this cell?"
//!
//! The vault cell's type script is xUDT itself.  This lock script is paired with the xUDT type
//! so that every vault cell carries the SUDT/xUDT token as its value layer.
//!
//! ## Args Layout (113 bytes):
//!
//!   [0..32]    `borrower_lock_hash`  – 32-byte lock hash of borrower wallet.
//!   [32..64]   `pool_type_hash`      – Type hash of the Liquidity Pool cell this vault is bound to.
//!   [64..80]   `principal`           – u128 LE: Asset B principal borrowed.
//!   [80..96]   `r_vault`             – u128 LE: frozen per-second interest rate (scaled by RATE_PRECISION).
//!   [96..104]  `t_created`           – u64  LE: vault creation timestamp (seconds).
//!   [104..112] `t_exp`               – u64  LE: vault expiration timestamp (seconds).
//!   [112]      `is_frozen`           – u8: 0 = active, 1 = frozen.
//!
//! Loan metadata lives in the args (not cell data) so that the cell data field
//! remains the standard 16-byte xUDT balance, preserving compatibility with any
//! custom xUDT collateral token regardless of its own data layout.
//!
//! ## Unlock Conditions:
//!
//! Regardless of which path is taken, the Liquidity Pool cell (identified by `pool_type_hash`)
//! MUST be present somewhere in the transaction inputs.  This unconditionally tethers the vault
//! to the pool so the pool-script's own validation always runs.
//!
//!   Path A — Early Repayment (Happy Path):
//!     Before the expiration timestamp the vault can ONLY be unlocked by the borrower.
//!     Proved by finding an input cell with lock_hash == borrower_lock_hash.
//!
//!   Path B — Liquidation (Default Path):
//!     If `t_now >= t_exp` (read from args) AND the pool is present,
//!     the vault is open to public liquidation without a borrower signature.

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
    high_level::{load_cell_lock_hash, load_cell_type_hash, load_header, load_script},
};

use lenderr_common::{errors::*, vault_data::VAULT_ARGS_LEN};

pub fn program_entry() -> i8 {
    // ── Load script args ─────────────────────────────────────────────────────
    let script = match load_script() {
        Ok(s) => s,
        Err(_) => return ERROR_SYSCALL,
    };

    let args: Bytes = script.args().unpack();

    // Require the full 113-byte args layout.
    if args.len() < VAULT_ARGS_LEN {
        return ERROR_ENCODING;
    }

    let mut borrower_lock_hash = [0u8; 32];
    borrower_lock_hash.copy_from_slice(&args[0..32]);

    let mut pool_type_hash = [0u8; 32];
    pool_type_hash.copy_from_slice(&args[32..64]);

    // Read t_exp from args[104..112].
    let mut t_exp_bytes = [0u8; 8];
    t_exp_bytes.copy_from_slice(&args[104..112]);
    let t_exp = u64::from_le_bytes(t_exp_bytes);

    // ── Unconditional: Pool Cell Must Be Present ──────────────────────────────
    //
    // Whether this is a repayment or a liquidation, the Liquidity Pool cell that
    // this vault belongs to MUST appear as an input in the transaction.
    // This forces the pool's type script to execute, which in turn validates all
    // the financial invariants (principal repaid, interest settled, etc.).
    if !pool_input_present(&pool_type_hash) {
        return ERROR_WRONG_POOL_RECIPIENT;
    }

    // ── Path A: Borrower Repayment (lock-hash presence auth) ──────────────────
    if borrower_is_present(&borrower_lock_hash) {
        return 0; // Borrower signed — authorised.
    }

    // ── Path B: Time-Based Liquidation ───────────────────────────────────────
    //
    // The borrower did not sign.  All loan terms, including `t_exp`, are stored
    // in the lock script args — uniform across all cells in this script group.
    // A single expiry check is therefore sufficient.
    let t_now = match load_header(0, Source::HeaderDep) {
        Ok(h) => h.raw().timestamp().unpack() / 1000, // ms → s
        Err(_) => return ERROR_SYSCALL,
    };

    if t_now < t_exp {
        return ERROR_VAULT_NOT_EXPIRED;
    }

    // Vault has expired and the pool is present — authorised for liquidation.
    0
}

/// Returns `true` if the transaction inputs contain a cell whose type hash matches `target`.
/// This binds the vault closure to the Liquidity Pool, ensuring the pool-script runs.
fn pool_input_present(target: &[u8; 32]) -> bool {
    let mut i = 0;
    loop {
        match load_cell_type_hash(i, Source::Input) {
            Ok(Some(ref hash)) if hash == target => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
        i += 1;
    }
}

/// Returns true if any input lock hash matches borrower_lock_hash.
fn borrower_is_present(borrower_lock_hash: &[u8; 32]) -> bool {
    let mut i = 0;
    loop {
        match load_cell_lock_hash(i, Source::Input) {
            Ok(h) => {
                if &h == borrower_lock_hash {
                    return true;
                }
            }
            Err(_) => return false,
        }
        i += 1;
    }
}
