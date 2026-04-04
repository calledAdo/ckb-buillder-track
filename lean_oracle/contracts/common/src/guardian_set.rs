//! This file defines the in-memory representation and manual byte encoding for
//! guardian-set cells.
//!
//! A guardian-set cell is the on-chain root of trust for Wormhole-style oracle
//! verification inside `lean_oracle`. The oracle script does not hardcode the
//! allowed signers; instead, it loads a governed guardian-set cell from
//! `CellDep` and checks recovered Ethereum-style guardian addresses against it.

// This struct is the decoded Rust view of a guardian-set cell's data section.
// Each field corresponds to a piece of state the oracle scripts care about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardianSetData {
    // Monotonic set index. This should increase when governance rotates signers.
    pub set_index: u32,
    // Minimum number of guardian signatures required for acceptance.
    pub quorum: u32,
    // Creation time of this set, expressed in seconds.
    pub creation_time: u64,
    // Expiry time of this set, expressed in seconds. A value of zero means
    // "active indefinitely until governance decides otherwise".
    pub expiration_time: u64,
    // Lock hash authorized to rotate this guardian set in the future.
    pub governance_lock_hash: [u8; 32],
    // The governed guardian addresses, stored in Ethereum 20-byte form.
    pub guardian_addresses: alloc::vec::Vec<[u8; 20]>,
}

// Header size in bytes before the variable-length guardian-address vector
// begins.
pub const GUARDIAN_SET_HEADER_LEN: usize = 60;

impl GuardianSetData {
    // Decode a guardian-set cell from raw bytes. Returning `None` means the
    // buffer was structurally invalid for this layout.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        // Reject buffers that cannot even contain the fixed-width header.
        if data.len() < GUARDIAN_SET_HEADER_LEN {
            return None;
        }

        // Read the set index from bytes [0..4).
        let mut set_index_bytes = [0u8; 4];
        set_index_bytes.copy_from_slice(&data[0..4]);

        // Read the quorum from bytes [4..8).
        let mut quorum_bytes = [0u8; 4];
        quorum_bytes.copy_from_slice(&data[4..8]);

        // Read the creation time from bytes [8..16).
        let mut creation_time_bytes = [0u8; 8];
        creation_time_bytes.copy_from_slice(&data[8..16]);

        // Read the expiration time from bytes [16..24).
        let mut expiration_time_bytes = [0u8; 8];
        expiration_time_bytes.copy_from_slice(&data[16..24]);

        // Read the governance lock hash from bytes [24..56).
        let mut governance_lock_hash = [0u8; 32];
        governance_lock_hash.copy_from_slice(&data[24..56]);

        // Read the guardian-count field from bytes [56..60).
        let mut guardian_count_bytes = [0u8; 4];
        guardian_count_bytes.copy_from_slice(&data[56..60]);
        // Convert the count into the host's `usize` so it can be used for
        // allocation and boundary checks.
        let guardian_count = u32::from_le_bytes(guardian_count_bytes) as usize;

        // Enforce exact total size: header plus N guardian addresses of 20 bytes
        // each. This keeps the layout strict and avoids trailing-garbage bugs.
        if data.len() != GUARDIAN_SET_HEADER_LEN + guardian_count * 20 {
            return None;
        }

        // Allocate the destination vector large enough to hold every guardian.
        let mut guardian_addresses = alloc::vec::Vec::with_capacity(guardian_count);
        // Start reading guardian addresses immediately after the fixed header.
        let mut cursor = GUARDIAN_SET_HEADER_LEN;
        // Keep scanning until all guardian bytes have been consumed.
        while cursor < data.len() {
            // Create a fixed-size 20-byte address buffer.
            let mut addr = [0u8; 20];
            // Copy one guardian address from the current cursor position.
            addr.copy_from_slice(&data[cursor..cursor + 20]);
            // Append the decoded guardian address to the vector.
            guardian_addresses.push(addr);
            // Advance to the next guardian address.
            cursor += 20;
        }

        // Return the fully decoded guardian-set struct.
        Some(Self {
            set_index: u32::from_le_bytes(set_index_bytes),
            quorum: u32::from_le_bytes(quorum_bytes),
            creation_time: u64::from_le_bytes(creation_time_bytes),
            expiration_time: u64::from_le_bytes(expiration_time_bytes),
            governance_lock_hash,
            guardian_addresses,
        })
    }

    // Encode the guardian-set struct back into the exact on-chain byte layout.
    pub fn to_bytes(&self) -> alloc::vec::Vec<u8> {
        // Preallocate enough space for the header plus every guardian address.
        let mut out =
            alloc::vec::Vec::with_capacity(GUARDIAN_SET_HEADER_LEN + self.guardian_addresses.len() * 20);
        // Write the set index in little-endian form.
        out.extend_from_slice(&self.set_index.to_le_bytes());
        // Write the quorum in little-endian form.
        out.extend_from_slice(&self.quorum.to_le_bytes());
        // Write the creation time in little-endian form.
        out.extend_from_slice(&self.creation_time.to_le_bytes());
        // Write the expiration time in little-endian form.
        out.extend_from_slice(&self.expiration_time.to_le_bytes());
        // Write the governance lock hash as-is.
        out.extend_from_slice(&self.governance_lock_hash);
        // Write the number of guardian addresses that follow.
        out.extend_from_slice(&(self.guardian_addresses.len() as u32).to_le_bytes());
        // Serialize each guardian address in order.
        for addr in &self.guardian_addresses {
            out.extend_from_slice(addr);
        }
        // Return the encoded byte vector.
        out
    }

    // Compare only the fields that represent governed guardian-set identity.
    // This helper is used by the guardian-set script to distinguish a pure
    // no-op update from an unauthorized mutation.
    pub fn governance_fields_unchanged(&self, other: &Self) -> bool {
        self.set_index == other.set_index
            && self.quorum == other.quorum
            && self.creation_time == other.creation_time
            && self.expiration_time == other.expiration_time
            && self.governance_lock_hash == other.governance_lock_hash
            && self.guardian_addresses == other.guardian_addresses
    }
}
