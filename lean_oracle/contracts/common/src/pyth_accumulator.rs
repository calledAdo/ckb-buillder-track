//! This file parses the raw Pyth accumulator update blob returned by Hermes.
//!
//! The parser implemented here follows the same high-level structure as the
//! Solidity-side Pyth accumulator verifier:
//! - parse the outer `PNAU` wrapper
//! - extract the embedded Wormhole VAA
//! - parse the Wormhole payload to get the signed Merkle root
//! - parse each attached price message and its proof
//! - verify that every decoded message is included in the signed root
//!
//! This is the bridge between "raw Hermes bytes" and "typed price updates that
//! the oracle cell can compare against its output state".

use crate::{
    merkle::verify_pyth_merkle_proof,
    wormhole_vaa::ParsedVaa,
};

// Magic for the outer Pyth accumulator wrapper (`PNAU`).
//
// This mirrors the upstream Solidity constant:
// - `ACCUMULATOR_MAGIC`
// - file: `PythAccumulator.sol`
pub const ACCUMULATOR_MAGIC: u32 = 0x504e4155;
// Magic for the Wormhole-merkle accumulator payload (`AUWV`).
//
// This mirrors the upstream Solidity constant:
// - `ACCUMULATOR_WORMHOLE_MAGIC`
// - file: `PythAccumulator.sol`
pub const ACCUMULATOR_WORMHOLE_MAGIC: u32 = 0x41555756;
// Supported outer accumulator major version.
//
// This mirrors the upstream Solidity constant:
// - `MAJOR_VERSION`
// - file: `PythAccumulator.sol`
pub const ACCUMULATOR_MAJOR_VERSION: u8 = 1;
// Supported update type inside the outer wrapper.
//
// This mirrors the upstream Solidity enum discriminant:
// - `UpdateType.WormholeMerkle`
// - file: `PythAccumulator.sol`
pub const UPDATE_TYPE_WORMHOLE_MERKLE: u8 = 0;
// Supported message type for price-feed messages.
//
// This mirrors the upstream Solidity enum discriminant:
// - `MessageType.PriceFeed`
// - file: `PythAccumulator.sol`
pub const MESSAGE_TYPE_PRICE_FEED: u8 = 0;

// Typed view of one decoded and authenticated Pyth price message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceFeedMessage {
    // Feed identifier for the price stream.
    pub feed_id: [u8; 32],
    // Spot price value.
    pub price: i64,
    // Spot confidence interval.
    pub conf: u64,
    // Decimal exponent.
    pub expo: i32,
    // Current publish time.
    pub publish_time: u64,
    // Previous publish time carried by the message.
    pub prev_publish_time: u64,
    // EMA price value.
    pub ema_price: i64,
    // EMA confidence interval.
    pub ema_conf: u64,
}

// Parsed view of a whole accumulator update batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedAccumulatorUpdate {
    // The embedded Wormhole VAA.
    pub vaa: ParsedVaa,
    // Slot number extracted from the Wormhole-merkle payload.
    pub slot: u64,
    // Signed 20-byte Merkle root extracted from the Wormhole payload.
    pub root_digest: [u8; 20],
    // All successfully parsed and Merkle-authenticated messages in the batch.
    pub messages: alloc::vec::Vec<PriceFeedMessage>,
}

// Parsed view of an accumulator update when the caller only cares about one
// target feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedAccumulatorUpdateForFeed {
    // The embedded Wormhole VAA.
    pub vaa: ParsedVaa,
    // Slot number extracted from the Wormhole-merkle payload.
    pub slot: u64,
    // Signed 20-byte Merkle root extracted from the Wormhole payload.
    pub root_digest: [u8; 20],
    // The single authenticated message for the requested feed.
    pub message: PriceFeedMessage,
}

impl ParsedAccumulatorUpdate {
    // Parse a full accumulator update from raw Hermes bytes.
    pub fn parse(data: &[u8]) -> Option<Self> {
        // Start at the beginning of the input buffer.
        let mut cursor = 0usize;
        // Read the outer magic value and require `PNAU`.
        let magic = read_u32_be(data, &mut cursor)?;
        if magic != ACCUMULATOR_MAGIC {
            return None;
        }

        // Read the major version and require the supported version.
        let major = read_u8(data, &mut cursor)?;
        if major != ACCUMULATOR_MAJOR_VERSION {
            return None;
        }

        // Read and ignore the minor version for now.
        let _minor = read_u8(data, &mut cursor)?;
        // Read the size of the optional trailing-header section.
        let trailing_header_size = read_u8(data, &mut cursor)? as usize;
        // Skip over that trailing-header section.
        take_bytes(data, &mut cursor, trailing_header_size)?;

        // Read the update type and require the Wormhole-merkle format.
        let update_type = read_u8(data, &mut cursor)?;
        if update_type != UPDATE_TYPE_WORMHOLE_MERKLE {
            return None;
        }

        // Everything after the fixed outer header is the encoded update body.
        let remaining = data.len().checked_sub(cursor)?;
        let encoded = take_bytes(data, &mut cursor, remaining)?;
        // Use a nested cursor for the encoded update body.
        let mut encoded_cursor = 0usize;

        // Read the byte length of the embedded Wormhole proof / VAA.
        let wh_proof_size = read_u16_be(encoded, &mut encoded_cursor)? as usize;
        // Slice the embedded VAA bytes.
        let vaa_bytes = take_bytes(encoded, &mut encoded_cursor, wh_proof_size)?;
        // Parse the embedded VAA.
        let vaa = ParsedVaa::parse(vaa_bytes)?;

        // Parse the VAA payload to get the signed slot and Merkle root.
        let (slot, root_digest) = parse_wormhole_merkle_payload(&vaa.payload)?;

        // Read how many authenticated price messages follow.
        let num_updates = read_u8(encoded, &mut encoded_cursor)? as usize;
        // Preallocate the destination vector.
        let mut messages = alloc::vec::Vec::with_capacity(num_updates);

        // Parse each message-plus-proof bundle in sequence.
        for _ in 0..num_updates {
            // Read the length of the encoded price message.
            let message_size = read_u16_be(encoded, &mut encoded_cursor)? as usize;
            // Slice the raw message bytes.
            let message_bytes = take_bytes(encoded, &mut encoded_cursor, message_size)?;
            // Read the proof length.
            let proof_size = read_u8(encoded, &mut encoded_cursor)? as usize;
            // Allocate storage for the proof nodes.
            let mut proof = alloc::vec::Vec::with_capacity(proof_size);
            // Parse every 20-byte proof node.
            for _ in 0..proof_size {
                let proof_item = take_bytes(encoded, &mut encoded_cursor, 20)?;
                let mut digest = [0u8; 20];
                digest.copy_from_slice(proof_item);
                proof.push(digest);
            }

            // Reject the update if the proof does not reconstruct the signed
            // Merkle root.
            if !verify_pyth_merkle_proof(root_digest, message_bytes, &proof) {
                return None;
            }

            // Parse the raw message bytes into a typed price message.
            let message = parse_price_feed_message(message_bytes)?;
            // Append the authenticated message to the batch.
            messages.push(message);
        }

        // Reject trailing bytes inside the encoded update body or the outer
        // wrapper to keep parsing strict.
        if encoded_cursor != encoded.len() || cursor != data.len() {
            return None;
        }

        // Return the fully parsed accumulator batch.
        Some(Self {
            vaa,
            slot,
            root_digest,
            messages,
        })
    }
}

impl ParsedAccumulatorUpdateForFeed {
    // Parse a full accumulator update from raw Hermes bytes, but only
    // materialize the authenticated message for the requested feed id.
    pub fn parse_for_feed(data: &[u8], target_feed_id: &[u8; 32]) -> Option<Self> {
        // Parse the same outer accumulator header as the full parser.
        let mut cursor = 0usize;
        let magic = read_u32_be(data, &mut cursor)?;
        if magic != ACCUMULATOR_MAGIC {
            return None;
        }

        let major = read_u8(data, &mut cursor)?;
        if major != ACCUMULATOR_MAJOR_VERSION {
            return None;
        }

        let _minor = read_u8(data, &mut cursor)?;
        let trailing_header_size = read_u8(data, &mut cursor)? as usize;
        take_bytes(data, &mut cursor, trailing_header_size)?;

        let update_type = read_u8(data, &mut cursor)?;
        if update_type != UPDATE_TYPE_WORMHOLE_MERKLE {
            return None;
        }

        // Parse the encoded update body.
        let remaining = data.len().checked_sub(cursor)?;
        let encoded = take_bytes(data, &mut cursor, remaining)?;
        let mut encoded_cursor = 0usize;

        let wh_proof_size = read_u16_be(encoded, &mut encoded_cursor)? as usize;
        let vaa_bytes = take_bytes(encoded, &mut encoded_cursor, wh_proof_size)?;
        let vaa = ParsedVaa::parse(vaa_bytes)?;
        let (slot, root_digest) = parse_wormhole_merkle_payload(&vaa.payload)?;

        let num_updates = read_u8(encoded, &mut encoded_cursor)? as usize;
        let mut matched_message: Option<PriceFeedMessage> = None;

        // Stream over the batch without materializing every message.
        for _ in 0..num_updates {
            let message_size = read_u16_be(encoded, &mut encoded_cursor)? as usize;
            let message_bytes = take_bytes(encoded, &mut encoded_cursor, message_size)?;
            let (message_type, feed_id) = parse_message_header(message_bytes)?;

            let proof_size = read_u8(encoded, &mut encoded_cursor)? as usize;

            // Unsupported message kinds are rejected even if they are not the
            // target feed. This preserves strictness about the batch shape.
            if message_type != MESSAGE_TYPE_PRICE_FEED {
                return None;
            }

            if &feed_id == target_feed_id {
                // Reject ambiguity if the batch contains the target feed more
                // than once.
                if matched_message.is_some() {
                    return None;
                }

                let mut proof = alloc::vec::Vec::with_capacity(proof_size);
                for _ in 0..proof_size {
                    let proof_item = take_bytes(encoded, &mut encoded_cursor, 20)?;
                    let mut digest = [0u8; 20];
                    digest.copy_from_slice(proof_item);
                    proof.push(digest);
                }

                if !verify_pyth_merkle_proof(root_digest, message_bytes, &proof) {
                    return None;
                }

                let message = parse_price_feed_message(message_bytes)?;
                matched_message = Some(message);
            } else {
                // For non-target messages we still consume the exact proof bytes
                // to keep parsing aligned, but we avoid allocating and
                // verifying proofs that do not affect this oracle cell.
                let proof_bytes_len = proof_size.checked_mul(20)?;
                take_bytes(encoded, &mut encoded_cursor, proof_bytes_len)?;
            }
        }

        if encoded_cursor != encoded.len() || cursor != data.len() {
            return None;
        }

        Some(Self {
            vaa,
            slot,
            root_digest,
            message: matched_message?,
        })
    }
}

// Parse the Wormhole payload format used by the Pyth accumulator path.
//
// This mirrors the upstream Solidity logic in:
// - `extractWormholeMerkleHeaderDigestAndNumUpdatesAndEncodedAndSlotFromAccumulatorUpdate(...)`
// - file: `PythAccumulator.sol`
fn parse_wormhole_merkle_payload(payload: &[u8]) -> Option<(u64, [u8; 20])> {
    // Start at the beginning of the Wormhole payload.
    let mut cursor = 0usize;
    // Require the expected `AUWV` magic.
    let magic = read_u32_be(payload, &mut cursor)?;
    if magic != ACCUMULATOR_WORMHOLE_MAGIC {
        return None;
    }

    // Require the expected Wormhole-merkle update type.
    let update_type = read_u8(payload, &mut cursor)?;
    if update_type != UPDATE_TYPE_WORMHOLE_MERKLE {
        return None;
    }

    // Read the slot number carried by the payload.
    let slot = read_u64_be(payload, &mut cursor)?;
    // Read and ignore the ring buffer size for now.
    let _ring_size = read_u32_be(payload, &mut cursor)?;

    // Read the 20-byte signed Merkle root digest.
    let mut digest = [0u8; 20];
    digest.copy_from_slice(take_bytes(payload, &mut cursor, 20)?);
    // Return the extracted slot and root digest.
    Some((slot, digest))
}

// Parse one raw price-feed message from the accumulator update body.
//
// This mirrors the upstream Solidity logic in:
// - `parsePriceFeedMessage(...)`
// - file: `PythAccumulator.sol`
fn parse_price_feed_message(data: &[u8]) -> Option<PriceFeedMessage> {
    // Start at the beginning of the message buffer.
    let mut cursor = 0usize;
    // Require the price-feed message type byte.
    let message_type = read_u8(data, &mut cursor)?;
    if message_type != MESSAGE_TYPE_PRICE_FEED {
        return None;
    }

    // Read the 32-byte feed id.
    let mut feed_id = [0u8; 32];
    feed_id.copy_from_slice(take_bytes(data, &mut cursor, 32)?);

    // Read the signed spot price.
    let price = i64::from_be_bytes(take_bytes(data, &mut cursor, 8)?.try_into().ok()?);
    // Read the confidence interval.
    let conf = u64::from_be_bytes(take_bytes(data, &mut cursor, 8)?.try_into().ok()?);
    // Read the exponent.
    let expo = i32::from_be_bytes(take_bytes(data, &mut cursor, 4)?.try_into().ok()?);
    // Read the publish time.
    let publish_time = u64::from_be_bytes(take_bytes(data, &mut cursor, 8)?.try_into().ok()?);
    // Read the previous publish time.
    let prev_publish_time =
        u64::from_be_bytes(take_bytes(data, &mut cursor, 8)?.try_into().ok()?);
    // Read the EMA price.
    let ema_price = i64::from_be_bytes(take_bytes(data, &mut cursor, 8)?.try_into().ok()?);
    // Read the EMA confidence interval.
    let ema_conf = u64::from_be_bytes(take_bytes(data, &mut cursor, 8)?.try_into().ok()?);

    // Reject trailing bytes so the message layout remains strict.
    if cursor != data.len() {
        return None;
    }

    // Return the decoded message.
    Some(PriceFeedMessage {
        feed_id,
        price,
        conf,
        expo,
        publish_time,
        prev_publish_time,
        ema_price,
        ema_conf,
    })
}

// Parse only the message type and feed id from an encoded price message.
// This lightweight helper lets the oracle stream through a batch and identify
// the one message it actually cares about before doing heavier work.
fn parse_message_header(data: &[u8]) -> Option<(u8, [u8; 32])> {
    let mut cursor = 0usize;
    let message_type = read_u8(data, &mut cursor)?;
    let mut feed_id = [0u8; 32];
    feed_id.copy_from_slice(take_bytes(data, &mut cursor, 32)?);
    Some((message_type, feed_id))
}

// Read a single byte from the current cursor position.
fn read_u8(data: &[u8], cursor: &mut usize) -> Option<u8> {
    Some(*take_bytes(data, cursor, 1)?.first()?)
}

// Read a big-endian `u16` from the current cursor position.
fn read_u16_be(data: &[u8], cursor: &mut usize) -> Option<u16> {
    Some(u16::from_be_bytes(take_bytes(data, cursor, 2)?.try_into().ok()?))
}

// Read a big-endian `u32` from the current cursor position.
fn read_u32_be(data: &[u8], cursor: &mut usize) -> Option<u32> {
    Some(u32::from_be_bytes(take_bytes(data, cursor, 4)?.try_into().ok()?))
}

// Read a big-endian `u64` from the current cursor position.
fn read_u64_be(data: &[u8], cursor: &mut usize) -> Option<u64> {
    Some(u64::from_be_bytes(take_bytes(data, cursor, 8)?.try_into().ok()?))
}

// Slice `len` bytes from the current cursor position and advance the cursor.
fn take_bytes<'a>(data: &'a [u8], cursor: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = cursor.checked_add(len)?;
    if end > data.len() {
        return None;
    }
    let out = &data[*cursor..end];
    *cursor = end;
    Some(out)
}
