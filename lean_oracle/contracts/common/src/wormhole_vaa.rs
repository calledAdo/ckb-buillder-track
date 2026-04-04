//! This file parses the Wormhole Verifiable Action Approval (VAA) envelope.
//!
//! The oracle only needs a subset of the full Wormhole protocol surface:
//! - guardian-set index
//! - guardian signatures
//! - emitter information
//! - sequence/timestamp metadata
//! - the raw payload body that Pyth uses for accumulator updates

// Each Wormhole signature entry occupies 66 bytes:
// - 1 byte guardian index
// - 65 bytes ECDSA signature
//
// This matches the Wormhole VAA format documentation:
// - VAA header contains `guardian_index || signature`
// - source: Wormhole docs, "VAAs"
pub const WORMHOLE_SIGNATURE_LEN: usize = 66;
// The ECDSA portion alone occupies 65 bytes: `r || s || v`.
//
// This also comes directly from the Wormhole VAA format.
pub const WORMHOLE_SIGNATURE_BYTES_LEN: usize = 65;
// The fixed-length prefix of a Wormhole body is 51 bytes before the arbitrary
// protocol payload begins.
//
// This is the sum of:
// - timestamp: 4
// - nonce: 4
// - emitter_chain: 2
// - emitter_address: 32
// - sequence: 8
// - consistency_level: 1
//
// That layout matches the Wormhole VAA body format documentation.
pub const WORMHOLE_BODY_PREFIX_LEN: usize = 51;

// Decoded signature entry from the Wormhole VAA header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WormholeSignature {
    // Which guardian in the active set produced this signature.
    pub guardian_index: u8,
    // The raw 65-byte signature bytes in `r || s || v` order.
    pub signature: [u8; WORMHOLE_SIGNATURE_BYTES_LEN],
}

impl WormholeSignature {
    // Extract the `r` component of the signature.
    pub fn r(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.signature[0..32]);
        out
    }

    // Extract the `s` component of the signature.
    pub fn s(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.signature[32..64]);
        out
    }

    // Extract the recovery id / `v` component of the signature.
    pub fn v(&self) -> u8 {
        self.signature[64]
    }
}

// Fully parsed Wormhole VAA envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedVaa {
    // VAA version byte.
    pub version: u8,
    // Active guardian-set index referenced by this message.
    pub guardian_set_index: u32,
    // All signatures attached to the VAA.
    pub signatures: alloc::vec::Vec<WormholeSignature>,
    // Wormhole body timestamp.
    pub timestamp: u32,
    // Wormhole body nonce.
    pub nonce: u32,
    // Wormhole emitter chain id.
    pub emitter_chain: u16,
    // Wormhole emitter address.
    pub emitter_address: [u8; 32],
    // Wormhole sequence number.
    pub sequence: u64,
    // Wormhole consistency level.
    pub consistency_level: u8,
    // Protocol-specific payload bytes after the 51-byte Wormhole body prefix.
    pub payload: alloc::vec::Vec<u8>,
    // The entire body bytes, preserved because Wormhole signatures cover this
    // exact region.
    pub body: alloc::vec::Vec<u8>,
}

impl ParsedVaa {
    // Parse a complete VAA from raw bytes.
    pub fn parse(encoded: &[u8]) -> Option<Self> {
        // Start reading from the beginning of the byte buffer.
        let mut cursor = 0usize;

        // Read the version byte.
        let version = *take_bytes(encoded, &mut cursor, 1)?.first()?;
        // Read the guardian-set index in big-endian format.
        let guardian_set_index = read_u32_be(encoded, &mut cursor)?;
        // Read the number of signatures in the header.
        let signature_count = *take_bytes(encoded, &mut cursor, 1)?.first()? as usize;

        // Allocate the signatures vector with the exact required capacity.
        let mut signatures = alloc::vec::Vec::with_capacity(signature_count);
        // Parse each signature entry in sequence.
        for _ in 0..signature_count {
            // Read the guardian index.
            let guardian_index = *take_bytes(encoded, &mut cursor, 1)?.first()?;
            // Read the raw 65-byte signature.
            let signature_bytes = take_bytes(encoded, &mut cursor, WORMHOLE_SIGNATURE_BYTES_LEN)?;
            // Copy the signature into a fixed-size array.
            let mut signature = [0u8; WORMHOLE_SIGNATURE_BYTES_LEN];
            signature.copy_from_slice(signature_bytes);
            // Push the decoded signature entry into the vector.
            signatures.push(WormholeSignature {
                guardian_index,
                signature,
            });
        }

        // Everything left in the VAA after the signature header is the signed
        // body.
        let remaining = encoded.len().checked_sub(cursor)?;
        let body = take_bytes(encoded, &mut cursor, remaining)?.to_vec();
        // Reject bodies that are too short to contain the fixed Wormhole prefix.
        if body.len() < WORMHOLE_BODY_PREFIX_LEN {
            return None;
        }

        // Decode the body timestamp.
        let timestamp = u32::from_be_bytes(body[0..4].try_into().ok()?);
        // Decode the body nonce.
        let nonce = u32::from_be_bytes(body[4..8].try_into().ok()?);
        // Decode the emitter chain id.
        let emitter_chain = u16::from_be_bytes(body[8..10].try_into().ok()?);

        // Decode the 32-byte emitter address.
        let mut emitter_address = [0u8; 32];
        emitter_address.copy_from_slice(&body[10..42]);

        // Decode the sequence number.
        let sequence = u64::from_be_bytes(body[42..50].try_into().ok()?);
        // Decode the consistency level byte.
        let consistency_level = body[50];
        // Everything after the first 51 bytes is protocol payload.
        let payload = body[WORMHOLE_BODY_PREFIX_LEN..].to_vec();

        // Return the parsed VAA.
        Some(Self {
            version,
            guardian_set_index,
            signatures,
            timestamp,
            nonce,
            emitter_chain,
            emitter_address,
            sequence,
            consistency_level,
            payload,
            body,
        })
    }
}

// Read a big-endian `u32` from the current cursor position.
fn read_u32_be(data: &[u8], cursor: &mut usize) -> Option<u32> {
    let bytes = take_bytes(data, cursor, 4)?;
    let mut out = [0u8; 4];
    out.copy_from_slice(bytes);
    Some(u32::from_be_bytes(out))
}

// Slice a fixed number of bytes from the current cursor position and advance.
fn take_bytes<'a>(data: &'a [u8], cursor: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = cursor.checked_add(len)?;
    if end > data.len() {
        return None;
    }
    let out = &data[*cursor..end];
    *cursor = end;
    Some(out)
}
