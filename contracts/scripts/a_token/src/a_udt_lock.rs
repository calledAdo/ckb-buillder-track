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

use alloc::vec;
use blake2b_ref::Blake2bBuilder;
#[cfg(not(test))]
use ckb_auth::{ckb_auth, AuthAlgorithmIdType, CkbAuthType, CkbEntryType, EntryCategoryType};
use ckb_std::{
    ckb_constants::Source,
    ckb_types::{bytes::Bytes, core::ScriptHashType, prelude::*, packed::WitnessArgs},
    high_level::{load_script,load_tx_hash, load_witness, load_input, QueryIter, load_cell_data, load_cell_type_hash},
};

// Error Codes
const ERROR_WITNESS_MISSING: i8 = -1;
const ERROR_SYSCALL: i8 = -2;
const ERROR_SIGNATURE_INVALID: i8 = -3;
const ERROR_DATA_MALFORMED: i8 = 51;

pub fn program_entry() -> i8 {
    // 1. Get the owner's pubkey_hash from this Lock Script's args
    let script = match load_script() {
        Ok(s) => s,
        Err(_) => return ERROR_SYSCALL,
    };
    let args: Bytes = script.args().unpack();
    
    // Standard Secp256k1 lock args are exactly 20 bytes (the pubkey hash)
    if args.len() < 20 {
        return ERROR_SIGNATURE_INVALID; 
    }
    
    // Extract the 20-byte expected pubkey hash
    let mut expected_pubkey_hash = [0u8; 20];
    expected_pubkey_hash.copy_from_slice(&args[0..20]);

    // ==========================================
    // STRATEGY A: Owner Signature Check
    // ==========================================
    if check_signature_from_witnesses(&expected_pubkey_hash).is_ok() {
        return 0; // Authorized by owner!
    }

    // ==========================================
    // STRATEGY B: The Allowance Fallback
    // ==========================================
    // If the signature failed, check if this is a delegated spend.
    
    // First, read the ID of the "Normal Token" this lock is currently protecting
    let current_cell_data = match load_cell_data(0, Source::GroupInput) {
        Ok(data) => data,
        Err(_) => return ERROR_DATA_MALFORMED,
    };
    
    if current_cell_data.len() < 49 {
        return ERROR_DATA_MALFORMED; // Doesn't match your 49-byte UDT structure
    }
    let target_token_id = &current_cell_data[17..49];

    // Next, get the Type Hash of the "Normal Token"
    // (If the token has no Type Script, we immediately fail because we have 
    // no cryptographic root of trust to verify the Allowance against).
    let target_type_hash = match load_cell_type_hash(0, Source::GroupInput) {
        Ok(Some(hash)) => hash,
        _ => return ERROR_ALLOWANCE_NOT_FOUND, 
    };

    // Run the scan through the inputs
    if check_allowance_in_inputs(target_token_id, &target_type_hash).is_ok() {
        return 0; // Authorized by a valid, contract-issued Allowance token!
    }

    // If both strategies fail, reject the transaction entirely
    ERROR_SIGNATURE_INVALID
}

/// Verifies that the transaction is signed by the owner of the expected pubkey_hash
pub fn check_signature_from_witnesses(expected_pubkey_hash: &[u8]) -> Result<(), i8> {
    #[cfg(test)]
    return Err(ERROR_SIGNATURE_INVALID);

    #[cfg(not(test))]
    {
        let (message, signature_bytes) = generate_sighash_all()?;

        let entry = CkbEntryType {
        code_hash: [0x00; 32], // TODO: Insert actual Secp256k1 system script hash
        // The unsafe transmute is a standard workaround for ckb-types version mismatch
        hash_type: unsafe { core::mem::transmute(1u8) }, // 1 = ScriptHashType::Type
        
        // Use Exec instead of DynamicLinking because DynamicLinking is not a variant, 
        // and DynamicLibrary currently crashes with ckb-std v0.18
        entry_category: EntryCategoryType::Exec, 
    };

    let mut auth = CkbAuthType {
        algorithm_id: AuthAlgorithmIdType::Ckb,
        pubkey_hash: [0u8; 20],
    };
    auth.pubkey_hash.copy_from_slice(&expected_pubkey_hash[0..20]);

        let result = ckb_auth(&entry, &auth, &signature_bytes, &message);

        match result {
            Ok(_) => Ok(()),
            Err(_) => Err(ERROR_SIGNATURE_INVALID),
        }
    }
}

/// Calculates the Blake2b Sighash of the transaction
#[cfg(not(test))]
fn generate_sighash_all() -> Result<([u8; 32], Bytes), i8> {
    let mut blake2b = Blake2bBuilder::new(32)
        .personal(b"ckb-default-hash")
        .build();

    // 1. Hash the Transaction Hash
    let tx_hash = load_tx_hash().map_err(|_| ERROR_SYSCALL)?;
    blake2b.update(&tx_hash);

    // 2. Hash the First Witness in the group (with a zeroed-out lock field)
    let witness_bytes = load_witness(0, Source::GroupInput).map_err(|_| ERROR_WITNESS_MISSING)?;
    let witness_args = WitnessArgs::from_slice(&witness_bytes).map_err(|_| ERROR_WITNESS_MISSING)?;
    
    // Extract the actual signature to return to ckb_auth later
    let lock_field = witness_args.lock().to_opt().ok_or(ERROR_WITNESS_MISSING)?;
    let signature_bytes: Bytes = lock_field.unpack();

    // Rebuild the WitnessArgs with zeroes in the lock field
    let zero_lock = ckb_std::ckb_types::packed::Bytes::new_builder()
        .set(vec![ckb_std::ckb_types::packed::Byte::new(0); signature_bytes.len()])
        .build();

    let lock_opt = ckb_std::ckb_types::packed::BytesOpt::new_builder()
        .set(Some(zero_lock))
        .build();

    let modified_witness_args = witness_args
        .as_builder()
        .lock(lock_opt)
        .build();

    let modified_witness_bytes = modified_witness_args.as_bytes();
    
    // Hash the length of the *entire* modified witness, then the witness itself
    let len_bytes = (modified_witness_bytes.len() as u64).to_le_bytes();
    blake2b.update(&len_bytes);
    blake2b.update(&modified_witness_bytes);

    // 3. Hash any additional witnesses in the same script group
    let mut i = 1;
    loop {
        match load_witness(i, Source::GroupInput) {
            Ok(witness) => {
                let len_bytes = (witness.len() as u64).to_le_bytes();
                blake2b.update(&len_bytes);
                blake2b.update(&witness);
            }
            Err(_) => break, // End of group witnesses
        }
        i += 1;
    }

    // 4. Hash tail witnesses (Witnesses that don't belong to any specific input)
    // First, dynamically count the total number of inputs in the transaction
    let mut total_inputs = 0;
    loop {
        if load_input(total_inputs, Source::Input).is_err() {
            break;
        }
        total_inputs += 1;
    }

    // Iterate starting from the total_inputs index
    let mut j = total_inputs; 
    loop {
        match load_witness(j, Source::Input) {
            Ok(witness) => {
                let len_bytes = (witness.len() as u64).to_le_bytes();
                blake2b.update(&len_bytes);
                blake2b.update(&witness);
            }
            Err(_) => break, // Reached the absolute end of all witnesses
        }
        j += 1;
    }

    let mut message = [0u8; 32];
    blake2b.finalize(&mut message);

    Ok((message, signature_bytes))
}


const ERROR_ALLOWANCE_NOT_FOUND: i8 = 50;

/// Scans all inputs for an Allowance token issued by the same UDT contract 
/// that matches the target token ID.
fn check_allowance_in_inputs(target_token_id: &[u8], expected_type_hash: &[u8; 32]) -> Result<(), i8> {
    // We use QueryIter to pull the data of every cell in the Input pool.
    // .enumerate() gives us the index `i` so we can look up its Type Hash later.
    let allowance_found = QueryIter::new(load_cell_data, Source::Input)
        .enumerate()
        .any(|(i, data)| {
            // 1. Structure Check: [1 byte Variant][16 bytes Amount][32 bytes Ref_ID]
            if data.len() < 49 {
                return false;
            }

            let variant = data[0];
            let ref_token_id = &data[17..49];

            // 2. Logic Check: Is it an Allowance variant targeting our token?
            if variant == 1 && ref_token_id == target_token_id {
                
                // 3. Security Check: Does this allowance actually belong to our UDT contract?
                // We use the index `i` to load the Type Hash of this specific input.
                if let Ok(Some(type_hash)) = load_cell_type_hash(i, Source::Input) {
                    if type_hash == *expected_type_hash {
                        return true; // We found a cryptographically valid permission slip!
                    }
                }
            }
            
            // If any check fails, move to the next input cell
            false
        });

    if allowance_found {
        Ok(())
    } else {
        Err(ERROR_ALLOWANCE_NOT_FOUND)
    }
}

