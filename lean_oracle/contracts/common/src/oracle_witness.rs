//! This file defines the binary witness wrapper used by oracle update
//! transactions.
//!
//! The current witness format is intentionally small:
//! - a single length prefix
//! - followed by one raw Pyth accumulator update blob
//!
//! The on-chain script is responsible for parsing that blob further.

// The decoded witness view used by the oracle script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleUpdateWitness {
    // Raw Pyth accumulator update bytes, usually fetched from Hermes.
    pub accumulator_update: alloc::vec::Vec<u8>,
}

impl OracleUpdateWitness {
    // Decode the witness wrapper from raw bytes.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        // Track the current parsing position.
        let mut cursor = 0usize;

        // Read the size prefix for the accumulator blob.
        let update_len = read_u32_le(data, &mut cursor)? as usize;
        // Slice out the accumulator blob itself.
        let accumulator_update = take_bytes(data, &mut cursor, update_len)?.to_vec();

        // Reject trailing bytes so the witness format stays strict.
        if cursor != data.len() {
            return None;
        }

        // Return the decoded witness.
        Some(Self { accumulator_update })
    }

    // Encode the witness back into the canonical binary format.
    pub fn to_bytes(&self) -> alloc::vec::Vec<u8> {
        // Create the output buffer.
        let mut out = alloc::vec::Vec::new();
        // Write the blob length prefix.
        out.extend_from_slice(&(self.accumulator_update.len() as u32).to_le_bytes());
        // Write the accumulator bytes themselves.
        out.extend_from_slice(&self.accumulator_update);
        // Return the encoded witness bytes.
        out
    }
}

// Read a little-endian `u32` from the current cursor position.
fn read_u32_le(data: &[u8], cursor: &mut usize) -> Option<u32> {
    // Slice the next four bytes from the buffer.
    let bytes = take_bytes(data, cursor, 4)?;
    // Move them into a fixed-size array for conversion.
    let mut out = [0u8; 4];
    out.copy_from_slice(bytes);
    // Convert the little-endian bytes into a `u32`.
    Some(u32::from_le_bytes(out))
}

// Slice `len` bytes from the current cursor position and advance the cursor.
fn take_bytes<'a>(data: &'a [u8], cursor: &mut usize, len: usize) -> Option<&'a [u8]> {
    // Compute the end position with overflow protection.
    let end = cursor.checked_add(len)?;
    // Reject requests that would run past the end of the buffer.
    if end > data.len() {
        return None;
    }
    // Borrow the requested byte range.
    let out = &data[*cursor..end];
    // Advance the cursor for the next parser step.
    *cursor = end;
    // Return the borrowed slice.
    Some(out)
}
