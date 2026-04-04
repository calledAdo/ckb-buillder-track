//! This file defines the in-memory representation and manual byte encoding for
//! oracle state cells.
//!
//! The oracle cell is the canonical on-chain storage object for a single feed.
//! It stores:
//! - the feed identifier
//! - the guardian-set dependency it trusts
//! - the latest authenticated price values
//! - source-emitter information

// This struct is the decoded Rust representation of a single oracle cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleData {
    // The 32-byte Pyth feed identifier this cell tracks.
    pub feed_id: [u8; 32],
    // The type hash of the guardian-set cell that the oracle must load from
    // `CellDep` when validating an update.
    pub guardian_set_type_hash: [u8; 32],
    // The latest signed spot price, kept in the same signed integer form used
    // by Pyth price messages.
    pub price: i64,
    // Confidence interval for the current spot price.
    pub conf: u64,
    // Decimal exponent applied to `price` and `conf`.
    pub expo: i32,
    // Time at which the current signed price was published.
    pub publish_time: u64,
    // The previous publish time that Pyth included in the signed update.
    pub prev_publish_time: u64,
    // EMA price carried by the signed update.
    pub ema_price: i64,
    // EMA confidence carried by the signed update.
    pub ema_conf: u64,
    // Wormhole guardian-set index that must match the parsed VAA.
    pub guardian_set_index: u32,
    // Wormhole emitter chain identifier that must match the parsed VAA.
    pub emitter_chain: u32,
    // Wormhole emitter address that must match the parsed VAA.
    pub emitter_address: [u8; 32],
}

// Total byte length of the oracle cell data layout.
pub const ORACLE_STATE_LEN: usize = 156;

impl OracleData {
    // Decode an oracle state struct from raw cell data.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        // Enforce the exact data length for this layout.
        if data.len() != ORACLE_STATE_LEN {
            return None;
        }

        // Read the feed identifier from bytes [0..32).
        let mut feed_id = [0u8; 32];
        feed_id.copy_from_slice(&data[0..32]);

        // Read the guardian-set type hash from bytes [32..64).
        let mut guardian_set_type_hash = [0u8; 32];
        guardian_set_type_hash.copy_from_slice(&data[32..64]);

        // Read the signed spot price from bytes [64..72).
        let mut price_bytes = [0u8; 8];
        price_bytes.copy_from_slice(&data[64..72]);

        // Read the confidence interval from bytes [72..80).
        let mut conf_bytes = [0u8; 8];
        conf_bytes.copy_from_slice(&data[72..80]);

        // Read the exponent from bytes [80..84).
        let mut expo_bytes = [0u8; 4];
        expo_bytes.copy_from_slice(&data[80..84]);

        // Read the publish time from bytes [84..92).
        let mut publish_time_bytes = [0u8; 8];
        publish_time_bytes.copy_from_slice(&data[84..92]);

        // Read the previous publish time from bytes [92..100).
        let mut prev_publish_time_bytes = [0u8; 8];
        prev_publish_time_bytes.copy_from_slice(&data[92..100]);

        // Read the EMA price from bytes [100..108).
        let mut ema_price_bytes = [0u8; 8];
        ema_price_bytes.copy_from_slice(&data[100..108]);

        // Read the EMA confidence from bytes [108..116).
        let mut ema_conf_bytes = [0u8; 8];
        ema_conf_bytes.copy_from_slice(&data[108..116]);

        // Read the guardian-set index from bytes [116..120).
        let mut guardian_set_index_bytes = [0u8; 4];
        guardian_set_index_bytes.copy_from_slice(&data[116..120]);

        // Read the emitter chain from bytes [120..124).
        let mut emitter_chain_bytes = [0u8; 4];
        emitter_chain_bytes.copy_from_slice(&data[120..124]);

        // Read the emitter address from bytes [124..156).
        let mut emitter_address = [0u8; 32];
        emitter_address.copy_from_slice(&data[124..156]);

        // Return the fully decoded oracle state.
        Some(Self {
            feed_id,
            guardian_set_type_hash,
            price: i64::from_le_bytes(price_bytes),
            conf: u64::from_le_bytes(conf_bytes),
            expo: i32::from_le_bytes(expo_bytes),
            publish_time: u64::from_le_bytes(publish_time_bytes),
            prev_publish_time: u64::from_le_bytes(prev_publish_time_bytes),
            ema_price: i64::from_le_bytes(ema_price_bytes),
            ema_conf: u64::from_le_bytes(ema_conf_bytes),
            guardian_set_index: u32::from_le_bytes(guardian_set_index_bytes),
            emitter_chain: u32::from_le_bytes(emitter_chain_bytes),
            emitter_address,
        })
    }

    // Encode the oracle state struct back into its exact byte layout.
    pub fn to_bytes(&self) -> alloc::vec::Vec<u8> {
        // Preallocate the full output size.
        let mut out = alloc::vec::Vec::with_capacity(ORACLE_STATE_LEN);
        // Write the feed identifier.
        out.extend_from_slice(&self.feed_id);
        // Write the guardian-set type hash.
        out.extend_from_slice(&self.guardian_set_type_hash);
        // Write the signed spot price.
        out.extend_from_slice(&self.price.to_le_bytes());
        // Write the confidence interval.
        out.extend_from_slice(&self.conf.to_le_bytes());
        // Write the exponent.
        out.extend_from_slice(&self.expo.to_le_bytes());
        // Write the publish time.
        out.extend_from_slice(&self.publish_time.to_le_bytes());
        // Write the previous publish time.
        out.extend_from_slice(&self.prev_publish_time.to_le_bytes());
        // Write the EMA price.
        out.extend_from_slice(&self.ema_price.to_le_bytes());
        // Write the EMA confidence.
        out.extend_from_slice(&self.ema_conf.to_le_bytes());
        // Write the guardian-set index.
        out.extend_from_slice(&self.guardian_set_index.to_le_bytes());
        // Write the emitter chain.
        out.extend_from_slice(&self.emitter_chain.to_le_bytes());
        // Write the emitter address.
        out.extend_from_slice(&self.emitter_address);
        // Return the encoded byte vector.
        out
    }

    // Compare only the configuration fields that should stay fixed during a
    // normal oracle price update.
    pub fn static_fields_unchanged(&self, other: &Self) -> bool {
        self.feed_id == other.feed_id
            && self.guardian_set_type_hash == other.guardian_set_type_hash
            && self.guardian_set_index == other.guardian_set_index
            && self.emitter_chain == other.emitter_chain
            && self.emitter_address == other.emitter_address
    }
}
