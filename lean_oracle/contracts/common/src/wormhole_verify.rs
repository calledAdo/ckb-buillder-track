//! This file contains the Wormhole guardian signature verifier.
//!
//! The verifier reconstructs the exact signing digest Wormhole uses:
//! `keccak256(keccak256(body))`
//!
//! It then:
//! - recovers a secp256k1 public key for each signature
//! - converts that public key into an Ethereum-style 20-byte address
//! - checks that address against the governed guardian-set cell
//! - enforces strictly increasing guardian indices so one guardian cannot be
//!   counted twice

use crate::{
    guardian_set::GuardianSetData,
    wormhole_vaa::ParsedVaa,
};
// Pull in the secp256k1 ECDSA recovery types we need.
use k256::{
    ecdsa::{RecoveryId, Signature, VerifyingKey},
};
// Pull in Keccak hashing for Wormhole digest reconstruction and Ethereum-style
// address derivation.
use sha3::{Digest, Keccak256};

// These are the semantic failure reasons for guardian verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyError {
    // Not enough signatures were supplied to satisfy the guardian-set quorum.
    Quorum,
    // Signature entries were not ordered by strictly increasing guardian index.
    Order,
    // A signature could not be recovered or did not match the expected guardian.
    Signature,
}

// Verify that a parsed Wormhole VAA carries a valid guardian quorum for the
// provided guardian set.
pub fn verify_guardian_quorum(
    vaa: &ParsedVaa,
    guardian_set: &GuardianSetData,
) -> Result<(), VerifyError> {
    // Reject VAAs that contain fewer signatures than the guardian-set quorum.
    if vaa.signatures.len() < guardian_set.quorum as usize {
        return Err(VerifyError::Quorum);
    }
    // Reject VAAs that somehow contain more signatures than the governed set
    // even has guardians.
    if vaa.signatures.len() > guardian_set.guardian_addresses.len() {
        return Err(VerifyError::Quorum);
    }

    // Wormhole signs `keccak256(keccak256(body))`, so we reconstruct exactly
    // that digest here.
    let body_hash = Keccak256::digest(&vaa.body);
    let digest = Keccak256::digest(body_hash);
    // Copy the digest into a fixed-size prehash buffer expected by `k256`.
    let mut prehash = [0u8; 32];
    prehash.copy_from_slice(&digest);

    // Track the last seen guardian index to enforce strict ordering and prevent
    // duplicate counting.
    let mut last_index: Option<u8> = None;
    // Verify each signature entry independently.
    for signature in &vaa.signatures {
        // If we already saw a guardian index, require the new one to be larger.
        if let Some(prev) = last_index {
            if signature.guardian_index <= prev {
                return Err(VerifyError::Order);
            }
        }
        // Update the ordering tracker.
        last_index = Some(signature.guardian_index);

        // Look up the expected 20-byte guardian address from the governed set.
        let expected = guardian_set
            .guardian_addresses
            .get(signature.guardian_index as usize)
            .ok_or(VerifyError::Signature)?;

        // Wormhole signatures may present `v` as 0/1 or 27/28 depending on the
        // environment. Normalize to 0/1 for `k256`.
        let normalized_v = match signature.v() {
            0 | 1 => signature.v(),
            27 | 28 => signature.v() - 27,
            _ => return Err(VerifyError::Signature),
        };

        // Convert the normalized recovery byte into a `RecoveryId`.
        let recid = RecoveryId::try_from(normalized_v).map_err(|_| VerifyError::Signature)?;
        // Parse the first 64 bytes of the signature as the low-level ECDSA
        // signature (`r || s`).
        let sig = Signature::from_slice(&signature.signature[..64]).map_err(|_| VerifyError::Signature)?;
        // Recover the verifying key from the prehash, signature, and recovery id.
        let recovered =
            VerifyingKey::recover_from_prehash(&prehash, &sig, recid).map_err(|_| VerifyError::Signature)?;
        // Convert the recovered secp256k1 key into an Ethereum-style address.
        let recovered_addr = ethereum_address(&recovered);
        // Reject if the recovered address does not match the governed guardian.
        if &recovered_addr != expected {
            return Err(VerifyError::Signature);
        }
    }

    // If we made it through every signature, the quorum is valid.
    Ok(())
}

// Convert a recovered secp256k1 public key into an Ethereum address.
pub fn ethereum_address(key: &VerifyingKey) -> [u8; 20] {
    // Encode the public key in uncompressed SEC1 format. This yields
    // `0x04 || x || y`.
    let encoded = key.to_encoded_point(false);
    // Borrow the encoded public-key bytes.
    let pubkey = encoded.as_bytes();
    // Ethereum addresses are derived from the Keccak hash of the 64-byte
    // uncompressed public key body, excluding the `0x04` prefix.
    let digest = Keccak256::digest(&pubkey[1..]);
    // Take the lower 20 bytes of the digest as the final address.
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest[12..32]);
    // Return the derived address.
    out
}
