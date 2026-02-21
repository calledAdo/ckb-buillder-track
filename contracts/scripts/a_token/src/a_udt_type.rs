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

use blake2b_ref::Blake2bBuilder;
use ckb_std::{
    ckb_constants::Source,
    ckb_types::prelude::*,
    high_level::{load_cell_capacity, load_cell_data, load_cell_lock_hash, load_input, load_script, QueryIter},
    syscalls::SysError,
};

// ==========================================
// ERROR CODES
// ==========================================
const ERROR_SYSCALL: i8 = -1;
const ERROR_DATA_MALFORMED: i8 = 51;
const ERROR_INSUFFICIENT_BALANCE: i8 = 52;
const ERROR_ALLOWANCE_NOT_BURNED: i8 = 53;
const ERROR_FORGED_VAULT_ID: i8 = 54;
const ERROR_REFUND_MISSING: i8 = 55;

pub fn program_entry() -> i8 {
    // 1. Load the Issuer's Lock Hash from this Type Script's args
    let script = match load_script() {
        Ok(s) => s,
        Err(_) => return ERROR_SYSCALL,
    };
    let args: ckb_std::ckb_types::bytes::Bytes = script.args().unpack();
    if args.len() < 20 {
        return ERROR_DATA_MALFORMED; 
    }
    
    let mut issuer_lock_hash = [0u8; 20];
    issuer_lock_hash.copy_from_slice(&args[0..20]);

    // 2. Issuer Bypass (Minting tokens out of thin air)
    if check_owner_mode(&issuer_lock_hash) {
        return 0; // The central issuer is allowed to mint freely
    }

    // ==========================================
    // USER MODE: VAULT VALIDATION ENGINE
    // ==========================================
    let mut input_amount = 0u128;
    let mut output_amount = 0u128;
    
    let mut input_allowances = 0;
    let mut output_allowances = 0;
    
    let mut burned_allowance_capacity = 0u64;
    let mut expected_refund_lock = [0u8; 20];

    // --- TALLY INPUTS ---
    let mut i = 0;
    loop {
        let data = match load_cell_data(i, Source::GroupInput) {
            Ok(d) => d,
            Err(SysError::IndexOutOfBound) => break,
            Err(_) => return ERROR_SYSCALL,
        };
        
        // Ensure the cell matches our 69-byte spec
        if data.len() < 69 { return ERROR_DATA_MALFORMED; }

        let variant = data[0];
        let mut amount_bytes = [0u8; 16];
        amount_bytes.copy_from_slice(&data[1..17]);
        
        input_amount += u128::from_le_bytes(amount_bytes);

        if variant == 1 {
            input_allowances += 1;
            // Capture the exact CKB capacity of the Allowance cell being burned
            burned_allowance_capacity += load_cell_capacity(i, Source::GroupInput).unwrap_or(0);
            // Capture the Lock Hash of the owner who deserves the refund
            expected_refund_lock.copy_from_slice(&data[49..69]);
        }
        i += 1;
    }

    // --- TALLY OUTPUTS ---
    for data in QueryIter::new(load_cell_data, Source::GroupOutput) {
        if data.len() < 69 { return ERROR_DATA_MALFORMED; }
        
        let variant = data[0];
        let mut amount_bytes = [0u8; 16];
        amount_bytes.copy_from_slice(&data[1..17]);
        
        output_amount += u128::from_le_bytes(amount_bytes);
        if variant == 1 { output_allowances += 1; }
    }

    // --- RULE 1: CONSERVATION OF BALANCE ---
    if input_amount < output_amount {
        return ERROR_INSUFFICIENT_BALANCE; 
    }

    // --- ROUTE THE TRANSACTION LIFECYCLE ---
    if input_allowances == 0 && output_allowances > 0 {
        // SCENARIO A: CREATING A VAULT
        // The user is minting a new Allowance. We must enforce the unforgeable Vault ID.
        let expected_vault_id = match calculate_expected_vault_id() {
            Ok(id) => id,
            Err(e) => return e,
        };
        
        for data in QueryIter::new(load_cell_data, Source::GroupOutput) {
            let vault_id = &data[17..49];
            // Any newly created vault cells (Variant 1) MUST use the generated ID
            if data[0] == 1 && vault_id != expected_vault_id {
                return ERROR_FORGED_VAULT_ID;
            }
        }

    } else if input_allowances > 0 {
        // SCENARIO B: SPENDING A VAULT
        // An allowance is being consumed. We must enforce the Burn and the Refund.
        
        if output_allowances > 0 {
            return ERROR_ALLOWANCE_NOT_BURNED; // No partial spends allowed!
        }

        // --- RULE 2: CKB CAPACITY REFUND ---
        let mut refund_found = false;
        let mut j = 0;
        loop {
            let lock_hash = match load_cell_lock_hash(j, Source::Output) {
                Ok(h) => h,
                Err(SysError::IndexOutOfBound) => break, // Reached end of all outputs
                Err(_) => return ERROR_SYSCALL,
            };
            
            // Look for any output cell belonging to the original owner
            if lock_hash[0..20] == expected_refund_lock {
                let capacity = load_cell_capacity(j, Source::Output).unwrap_or(0);
                // Ensure the delegate returned enough CKB to cover the state rent
                if capacity >= burned_allowance_capacity {
                    refund_found = true;
                    break;
                }
            }
            j += 1;
        }

        if !refund_found {
            return ERROR_REFUND_MISSING; // Delegate tried to steal the CKB rent!
        }
    }

    // If it reaches here (including Scenario C: Normal Transfer), validation passed.
    0 
}

/// Checks if the Issuer's Lock Hash is present in any of the Input cells
fn check_owner_mode(issuer_lock_hash: &[u8; 20]) -> bool {
    QueryIter::new(load_cell_lock_hash, Source::Input)
        .any(|lock_hash| &lock_hash[0..20] == issuer_lock_hash)
}

/// Calculates the guaranteed unique Vault ID from Input 0's OutPoint
fn calculate_expected_vault_id() -> Result<[u8; 32], i8> {
    let first_input = load_input(0, Source::Input).map_err(|_| ERROR_SYSCALL)?;
    
    let mut blake2b = Blake2bBuilder::new(32).personal(b"ckb-default-hash").build();
    blake2b.update(first_input.previous_output().as_slice());
    
    let mut expected_vault_id = [0u8; 32];
    blake2b.finalize(&mut expected_vault_id);
    
    Ok(expected_vault_id)
}