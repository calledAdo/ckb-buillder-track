//! LendNerv Utilities (`utils.rs`)
//!
//! Technical Overview:
//! This module provides a collection of shared utility functions used across
//! the LendNerv smart contracts. Its primary purpose is to abstract away repetitive
//! operations on the Nervos cell model, such as decoding sUDT (Simple User Defined Token)
//! data payloads safely. Because LendNerv interacts extensively with wrapped assets
//! like USDC and WCKB, strict adherence to sUDT parsing rules is critical for security.

/// Parses the u128 token amount from the data field of an sUDT cell.
///
/// In the Nervos sUDT standard, the user's token balance is encoded as a
/// 16-byte strictly little-endian `u128` explicitly at the very beginning
/// of the cell's `data` field.
///
/// Returns `None` if the data slice is too short to contain a valid balance.
pub fn read_sudt_amount(data: &[u8]) -> Option<u128> {
    // Check if the data array has at least 16 bytes (the required length for an sUDT amount).
    // This prevents out-of-bounds panics when reading malformed cell data.
    if data.len() < 16 {
        return None;
    }

    // Create a 16-byte fixed-size buffer initialized with zeros.
    // This will hold the little-endian bytes extracted from the cell data.
    let mut buf = [0u8; 16];

    // Copy the first 16 bytes from the cell data directly into the buffer.
    // We can safely panic here if slicing was wrong, but the above length check ensures safety.
    buf.copy_from_slice(&data[0..16]);

    // Construct and return the absolute u128 mathematical value using little-endian layout.
    // This matches the standard definition of all sUDT amounts on Nervos Network.
    Some(u128::from_le_bytes(buf))
}
