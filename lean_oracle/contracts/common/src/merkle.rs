//! This file contains the hash and Merkle-proof helpers used by the oracle
//! verifier.
//!
//! There are two related but distinct proof systems here:
//! - a generic Keccak-based 32-byte Merkle helper used for tests and utilities
//! - the exact 20-byte Pyth accumulator Merkle scheme used by authenticated
//!   price-feed messages

// Import the digest trait and the Keccak implementation used throughout the
// oracle project.
use sha3::{Digest, Keccak256};

// Compute a standard Keccak-256 hash and return the full 32-byte digest.
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    // Hash the input bytes with Keccak-256.
    let digest = Keccak256::digest(data);
    // Copy the digest into a fixed-size array so callers can use a stable
    // `[u8; 32]` result type.
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    // Return the fixed-size digest.
    out
}

// Verify a standard 32-byte Merkle proof where sibling ordering is normalized
// lexicographically at each step.
pub fn verify_merkle_proof(root: [u8; 32], leaf_hash: [u8; 32], proof: &[[u8; 32]]) -> bool {
    // Start the rolling hash at the provided leaf hash.
    let mut current = leaf_hash;
    // Fold each sibling into the rolling hash one level at a time.
    for sibling in proof {
        // Build a temporary 64-byte buffer for `current || sibling` or
        // `sibling || current`, depending on their order.
        let mut combined = [0u8; 64];
        // Order the pair deterministically so the proof format does not depend
        // on callers remembering "left" versus "right" positions.
        if current <= *sibling {
            combined[..32].copy_from_slice(&current);
            combined[32..].copy_from_slice(sibling);
        } else {
            combined[..32].copy_from_slice(sibling);
            combined[32..].copy_from_slice(&current);
        }
        // Hash the ordered pair to move one level up the tree.
        current = keccak256(&combined);
    }
    // The proof is valid only if the final reconstructed node equals the root.
    current == root
}

// Compute the exact Pyth leaf hash for an encoded price message.
//
// Pyth prefixes leaf payloads with a discriminator byte `0` before hashing and
// truncates the Keccak output to 20 bytes.
//
// This mirrors the upstream Solidity rule in:
// - `MerkleTree.leafHash(...)`
// - file: `target_chains/ethereum/contracts/contracts/libraries/MerkleTree.sol`
pub fn pyth_leaf_hash(data: &[u8]) -> [u8; 20] {
    // Build `0 || leaf_bytes`.
    let mut payload = alloc::vec::Vec::with_capacity(1 + data.len());
    payload.push(0);
    payload.extend_from_slice(data);
    // Hash the prefixed payload with Keccak.
    let digest = Keccak256::digest(&payload);
    // Truncate the digest to the first 20 bytes, matching Pyth's format.
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest[..20]);
    // Return the truncated hash.
    out
}

// Hash a pair of 20-byte Pyth Merkle nodes into their parent node.
//
// This mirrors the upstream Solidity rule in:
// - `MerkleTree.nodeHash(...)`
// - file: `target_chains/ethereum/contracts/contracts/libraries/MerkleTree.sol`
pub fn pyth_node_hash(left: [u8; 20], right: [u8; 20]) -> [u8; 20] {
    // Order the two children lexicographically so the proof format remains
    // stable and position-independent.
    let (a, b) = if left <= right { (left, right) } else { (right, left) };
    // Build the exact Pyth internal-node payload:
    // `1 || min(child_a, child_b) || max(child_a, child_b)`.
    let mut payload = [0u8; 41];
    payload[0] = 1;
    payload[1..21].copy_from_slice(&a);
    payload[21..41].copy_from_slice(&b);
    // Hash the internal-node payload.
    let digest = Keccak256::digest(payload);
    // Truncate the digest to 20 bytes, again matching Pyth's format.
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest[..20]);
    // Return the parent node.
    out
}

// Verify a Pyth accumulator Merkle proof using the exact 20-byte hash format.
//
// This mirrors the upstream Solidity proof path in:
// - `MerkleTree.isProofValid(...)`
// - file: `target_chains/ethereum/contracts/contracts/libraries/MerkleTree.sol`
pub fn verify_pyth_merkle_proof(root: [u8; 20], leaf_data: &[u8], proof: &[[u8; 20]]) -> bool {
    // Start from the authenticated leaf hash.
    let mut current = pyth_leaf_hash(leaf_data);
    // Fold each 20-byte sibling into the rolling hash.
    for sibling in proof {
        current = pyth_node_hash(current, *sibling);
    }
    // The proof is valid only if the reconstructed node equals the signed root.
    current == root
}
