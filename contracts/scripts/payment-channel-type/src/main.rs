//! # Payment Channel Type Script
//!
//! This script owns the channel state machine.
//!
//! The companion UDT escrow cell is intentionally locked elsewhere. This state
//! cell exists so the channel can be looked up directly by `channel_id` through
//! its type args while keeping UDT unchanged on the escrow cell.
//!
//! The intended cell pair is:
//! - a tiny state cell with `always_success` lock and this type script
//! - a UDT escrow cell with `cell-owned-lock`
//!
//! The state type script enforces:
//! - channel identity immutability
//! - buyer-signed dispute updates
//! - strict monotonicity after dispute start
//! - cooperative-close signature checks
//! - post-dispute payout maturity via input `since`
//! - final payout correctness across all linked escrow cells
//!    
//! The escrow lock only enforces:
//! - the escrow cell cannot move without the matching state cell
//!
//! ## Type args
//!
//! The type args are exactly one 32-byte `channel_id`.
//!
//! On creation, that `channel_id` must equal:
//!
//! `blake2b("ckb-default-hash", first_input_tx_hash || first_input_index_LE4 || output_index_LE4)`
//!
//! where:
//! - `first_input_tx_hash || first_input_index_LE4` is the previous outpoint of
//!   the transaction's first input cell
//! - `output_index` is this state cell's actual output index in the transaction
//!
//! ## Data layout (160 bytes)
//!
//! ```text
//! [0..20)    seller_blake160          seller identity
//! [20..40)   buyer_blake160           buyer identity
//! [40..72)   payout_lock_code_hash    code hash for seller/buyer payout locks
//! [72..104)  escrow_lock_code_hash    code hash expected on linked escrow cells
//! [104..136) udt_type_hash            type hash of the UDT token used by this channel
//! [136..137) dispute_started          u8, 0=open, 1=disputed
//! [137..144) reserved                 must be zero
//! [144..160) seller_claim_udt         u128 LE cumulative claim
//! ```
//!
//! ## Witness modes
//!
//! The state script uses the same witness lock conventions as the old single
//! contract design so off-chain signing semantics remain familiar.
//!
//! - `130` bytes: cooperative close
//!   - `buyer_sig(65) || seller_sig(65)`
//! - `65` bytes: start dispute / challenge
//!   - `buyer_sig(65)`
//! - `0` bytes: post-dispute close after the payout input `since` matured

#![cfg_attr(not(any(feature = "library", test)), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(any(feature = "library", test))]
extern crate alloc;

mod error;

use core::convert::TryInto;
use core::result::Result::{self, Err, Ok};

use ckb_std::ckb_types::prelude::*;
use ckb_std::{
    ckb_constants::Source,
    error::SysError,
    high_level::{
        load_cell_data, load_cell_lock, load_cell_type_hash, load_input_since, load_script,
        load_witness_args,
    },
    since::{LockValue, Since},
};

// Re-export the local error enum into this module's namespace so every helper
// can return the same contract-specific failure type.
use error::Error;

// The state cell is looked up by channel id, so the type args must be exactly
// one 32-byte identifier.
const CHANNEL_ID_LEN: usize = 32;
// Byte offsets for the immutable identity region.
const SELLER_BLAKE160_OFFSET: usize = 0;
const BUYER_BLAKE160_OFFSET: usize = 20;
const PAYOUT_LOCK_CODE_HASH_OFFSET: usize = 40;
const ESCROW_LOCK_CODE_HASH_OFFSET: usize = 72;
const UDT_TYPE_HASH_OFFSET: usize = 104;
// Byte offsets for the mutable dispute region.
const DISPUTE_STARTED_OFFSET: usize = 136;
const SELLER_CLAIM_OFFSET: usize = 144;
// The full state cell data payload is fixed-size and easy to validate.
const STATE_DATA_LEN: usize = 160;

// Recovered secp256k1 signatures are always 65 bytes in `r || s || recid` form.
const SIG_LEN: usize = 65;
// Cooperative close needs one buyer signature and one seller signature.
const COOP_WITNESS_LEN: usize = SIG_LEN * 2;
// Dispute update now carries only the buyer signature; the proposed claim is
// read from the output state cell itself.
const DISPUTE_WITNESS_LEN: usize = SIG_LEN;
// Disputes use a resettable 48-hour payout delay enforced via input `since`.
const DISPUTE_WINDOW_SECS: u64 = 48 * 60 * 60;
const DISPUTE_WINDOW_MILLIS: u64 = DISPUTE_WINDOW_SECS * 1000;

#[derive(Clone, Copy)]
struct StateData {
    // Immutable identity of the seller entitled to payouts.
    seller_blake160: [u8; 20],
    // Immutable identity of the buyer that authorizes tickets.
    buyer_blake160: [u8; 20],
    // The standard payout lock code hash we expect seller/buyer outputs to use.
    payout_lock_code_hash: [u8; 32],
    // The escrow lock code hash used by all linked escrow cells.
    escrow_lock_code_hash: [u8; 32],
    // The UDT type hash that linked escrow cells must use.
    udt_type_hash: [u8; 32],
    // False while open, true once dispute has started.
    dispute_started: bool,
    // Cumulative seller claim authorized by the buyer.
    seller_claim_udt: u128,
}

#[cfg(not(any(feature = "library", test)))]
ckb_std::entry!(program_entry);
#[cfg(not(any(feature = "library", test)))]
ckb_std::default_alloc!(16384, 1258306, 64);

pub fn program_entry() -> i8 {
    // Convert `Result<(), Error>` into the raw i8 code CKB expects.
    match run() {
        Ok(()) => 0,
        Err(e) => e as i8,
    }
}

fn run() -> Result<(), Error> {
    // Load the currently executing type script so we can inspect its args.
    let script = load_script()?;
    // The args are the stable channel id for this state cell.
    let channel_id = script.args().raw_data();
    // Reject malformed args immediately so later helpers can assume layout.
    if channel_id.len() != CHANNEL_ID_LEN {
        return Err(Error::InvalidArgsLength);
    }

    // Count how many times this exact full type script appears on the input
    // side and output side. The state machine only supports one live state
    // cell per channel id, so every legal transition must have cardinality 0
    // or 1 on each side.
    let script_hash: [u8; 32] = script.calc_script_hash().unpack();
    let input_count = count_type_hash_matches(&script_hash, Source::Input)?;
    let output_count = count_type_hash_matches(&script_hash, Source::Output)?;

    // The state cell therefore has only three legal shapes:
    // - create:  absent -> present
    // - update:  present -> present
    // - destroy: present -> absent
    match (input_count, output_count) {
        (0, 1) => create_channel_state(channel_id.as_ref()),
        (1, 1) => update_channel_state(channel_id.as_ref()),
        (1, 0) => destroy_channel_state(channel_id.as_ref()),
        _ => Err(Error::InvalidGroupShape),
    }
}

fn find_current_output_index(script_hash: &[u8; 32]) -> Result<u32, Error> {
    // Return the unique transaction output whose type-script hash equals the
    // currently executing state type script hash. Duplicate matches are
    // rejected so channel-id uniqueness is enforced on chain rather than only
    // assumed by off-chain code.
    let mut i = 0usize;
    let mut matched_index: Option<u32> = None;

    loop {
        match load_cell_type_hash(i, Source::Output) {
            Ok(Some(type_hash)) if type_hash.as_slice() == script_hash => {
                if matched_index.is_some() {
                    return Err(Error::InvalidChannelId);
                }
                matched_index = Some(i as u32);
            }
            Ok(_) => {}
            Err(SysError::IndexOutOfBound) => return matched_index.ok_or(Error::InvalidChannelId),
            Err(e) => return Err(e.into()),
        }

        i += 1;
    }
}

fn count_type_hash_matches(script_hash: &[u8; 32], source: Source) -> Result<usize, Error> {
    // Count exact matches of this full type script hash on one side of the
    // transaction.
    let mut i = 0usize;
    let mut count = 0usize;

    loop {
        match load_cell_type_hash(i, source) {
            Ok(Some(type_hash)) if type_hash.as_slice() == script_hash => count += 1,
            Ok(_) => {}
            Err(SysError::IndexOutOfBound) => return Ok(count),
            Err(e) => return Err(e.into()),
        }

        i += 1;
    }
}

fn create_channel_state(channel_id: &[u8]) -> Result<(), Error> {
    // Read the first transaction input cell's previous outpoint:
    // `tx_hash(32) || index_LE4(4)`.
    //
    // This repo uses that previous outpoint, plus the state cell's own output
    // index, as the stable channel-id derivation payload.
    let mut input_buf = [0u8; 36];
    let _ = ckb_std::syscalls::load_input(&mut input_buf, 0, 0, Source::Input)
        .map_err(|_| Error::Encoding)?;

    let script_hash: [u8; 32] = load_script()?.calc_script_hash().unpack();
    let output_index = find_current_output_index(&script_hash)?;

    // Enforce that the type args really are the derived channel id.
    if channel_id != hash_outpoint(&input_buf, output_index).as_ref() {
        return Err(Error::InvalidChannelId);
    }

    // Parse the single output state cell so we can validate initial values.
    let state = parse_state_data(&load_cell_data(0, Source::GroupOutput)?)?;
    // Fresh channels must start open. That means all mutable dispute fields are
    // zeroed until the first dispute update is submitted.
    if state.dispute_started || state.seller_claim_udt != 0 {
        return Err(Error::InitialStateMustBeOpen);
    }
    Ok(())
}

fn update_channel_state(channel_id: &[u8]) -> Result<(), Error> {
    // Load raw input/output bytes first so we can both decode them and compare
    // the output against the exact bytes we expect later.
    let input_raw = load_cell_data(0, Source::GroupInput)?;
    let output_raw = load_cell_data(0, Source::GroupOutput)?;
    // Decode both states into structured fields.
    let input = parse_state_data(&input_raw)?;
    let output = parse_state_data(&output_raw)?;

    // The identity region of the channel state cell is immutable forever.
    if input.seller_blake160 != output.seller_blake160
        || input.buyer_blake160 != output.buyer_blake160
        || input.payout_lock_code_hash != output.payout_lock_code_hash
        || input.escrow_lock_code_hash != output.escrow_lock_code_hash
        || input.udt_type_hash != output.udt_type_hash
    {
        return Err(Error::IdentityImmutable);
    }

    let state_type_hash = load_script()?.calc_script_hash();
    // Channel-state update transactions do not carry escrow assets. Those
    // escrow cells only appear when closing the channel.
    let input_escrow_total = sum_linked_escrows(
        input.escrow_lock_code_hash.as_slice(),
        input.udt_type_hash.as_slice(),
        state_type_hash.as_slice(),
        Source::Input,
    )?;
    let output_escrow_total = sum_linked_escrows(
        input.escrow_lock_code_hash.as_slice(),
        input.udt_type_hash.as_slice(),
        state_type_hash.as_slice(),
        Source::Output,
    )?;

    if input_escrow_total != 0 || output_escrow_total != 0 {
        return Err(Error::EscrowAccountingMismatch);
    }

    // Read the state-script witness lock bytes for the dispute payload.
    let witness = load_group_witness_lock()?;
    if witness.len() != DISPUTE_WITNESS_LEN {
        return Err(Error::InvalidWitness);
    }

    // The proposed new seller claim comes from the output state cell, not the
    // witness. The witness only carries the buyer signature authorizing it.
    let new_seller_claim = output.seller_claim_udt;
    let buyer_sig = &witness[..];

    // Verify the buyer really signed `(claim, channel_id)`.
    verify_buyer_ticket(
        &input.buyer_blake160,
        channel_id,
        new_seller_claim,
        buyer_sig,
    )?;

    // This is a strictly one-way channel, so the claim must always increase.
    if new_seller_claim <= input.seller_claim_udt {
        return Err(Error::TicketNotHigherThanCurrent);
    }

    if !output.dispute_started {
        return Err(Error::InvalidStateTransition);
    }

    Ok(())
}

fn destroy_channel_state(channel_id: &[u8]) -> Result<(), Error> {
    // Read the final input state being closed.
    let input = parse_state_data(&load_cell_data(0, Source::GroupInput)?)?;
    let state_type_hash = load_script()?.calc_script_hash();
    let input_escrow_total = sum_linked_escrows(
        input.escrow_lock_code_hash.as_slice(),
        input.udt_type_hash.as_slice(),
        state_type_hash.as_slice(),
        Source::Input,
    )?;
    let output_escrow_total = sum_linked_escrows(
        input.escrow_lock_code_hash.as_slice(),
        input.udt_type_hash.as_slice(),
        state_type_hash.as_slice(),
        Source::Output,
    )?;
    if output_escrow_total != 0 {
        return Err(Error::EscrowAccountingMismatch);
    }
    let (seller_out, buyer_out) = collect_payout_outputs(
        &input.seller_blake160,
        &input.buyer_blake160,
        &input.payout_lock_code_hash,
        &input.udt_type_hash,
    )?;
    if seller_out + buyer_out != input_escrow_total {
        return Err(Error::EscrowAccountingMismatch);
    }
    // The witness tells us whether this is a cooperative close or a post-dispute close.
    let witness = load_group_witness_lock()?;

    // Open channels can only be closed cooperatively.
    if !input.dispute_started {
        if witness.len() != COOP_WITNESS_LEN {
            return Err(Error::InvalidWitness);
        }
        // Validate both signatures against the actual seller/buyer payout split.
        verify_cooperative_close(channel_id, &input, &witness)?;
        return Ok(());
    }

    // Once disputed, cooperative witness format is no longer accepted.
    if witness.len() == COOP_WITNESS_LEN {
        return Err(Error::AlreadyInDispute);
    }
    // Post-dispute close uses an empty witness lock.
    if !witness.is_empty() {
        return Err(Error::InvalidWitness);
    }

    // Payout maturity is enforced by the state-cell input's relative timestamp
    // `since` value, not by comparing against a chosen header time.
    require_payout_since(0, Source::GroupInput)?;
    if seller_out != input.seller_claim_udt {
        return Err(Error::EscrowAccountingMismatch);
    }

    Ok(())
}

fn verify_cooperative_close(
    channel_id: &[u8],
    state: &StateData,
    witness: &[u8],
) -> Result<(), Error> {
    // Split the 130-byte witness into buyer and seller signatures.
    let buyer_sig = &witness[..SIG_LEN];
    let seller_sig = &witness[SIG_LEN..];
    // Read the actual payout outputs from the transaction itself.
    let (seller_udt, buyer_udt) = collect_payout_outputs(
        &state.seller_blake160,
        &state.buyer_blake160,
        &state.payout_lock_code_hash,
        &state.udt_type_hash,
    )?;

    // Cooperative close signs the exact final split, not just some abstract
    // "close now" intent. That binds the signatures to the real payout outputs.
    let mut payload = [0u8; 16 + 16 + CHANNEL_ID_LEN];
    payload[..16].copy_from_slice(&seller_udt.to_le_bytes());
    payload[16..32].copy_from_slice(&buyer_udt.to_le_bytes());
    payload[32..].copy_from_slice(channel_id);
    let msg = blake2b_256(&payload);

    // Both parties must approve the same exact split.
    verify_secp256k1(&state.buyer_blake160, buyer_sig, &msg)?;
    verify_secp256k1(&state.seller_blake160, seller_sig, &msg)?;
    Ok(())
}

fn verify_buyer_ticket(
    buyer_blake160: &[u8; 20],
    channel_id: &[u8],
    seller_claim_udt: u128,
    buyer_sig: &[u8],
) -> Result<(), Error> {
    // Dispute updates sign `(claim, channel_id)` so tickets are tied to
    // exactly one channel and one cumulative claim.
    let mut payload = [0u8; 16 + CHANNEL_ID_LEN];
    payload[..16].copy_from_slice(&seller_claim_udt.to_le_bytes());
    payload[16..].copy_from_slice(channel_id);
    let msg = blake2b_256(&payload);
    verify_secp256k1(buyer_blake160, buyer_sig, &msg)
}

fn load_group_witness_lock() -> Result<ckb_std::ckb_types::bytes::Bytes, Error> {
    // Load the first group input's WitnessArgs because that is where CKB puts
    // script-specific witness data for this script group.
    let witness_args = load_witness_args(0, Source::GroupInput)?;
    // Extract the `lock` field because our witness protocol stores everything there.
    witness_args
        .lock()
        .to_opt()
        .ok_or(Error::WitnessLockMissing)
        .map(|bytes| bytes.raw_data())
}

fn require_payout_since(index: usize, source: Source) -> Result<(), Error> {
    // The payout transaction must spend the disputed state cell with a
    // relative timestamp-based `since` of at least 48 hours. Consensus
    // enforces the actual maturity. This script only checks that the right
    // kind of `since` was supplied.
    let raw_since = load_input_since(index, source)?;
    let since = Since::new(raw_since);

    if !since.flags_is_valid() || !since.is_relative() {
        return Err(Error::InvalidPayoutSince);
    }

    match since.extract_lock_value() {
        Some(LockValue::Timestamp(milliseconds)) if milliseconds >= DISPUTE_WINDOW_MILLIS => Ok(()),
        _ => Err(Error::InvalidPayoutSince),
    }
}

fn parse_state_data(data: &[u8]) -> Result<StateData, Error> {
    // State cell data is fixed-size so any mismatch means the layout is broken.
    if data.len() != STATE_DATA_LEN {
        return Err(Error::InvalidStateDataLength);
    }
    if data[DISPUTE_STARTED_OFFSET] > 1
        || data[DISPUTE_STARTED_OFFSET + 1..SELLER_CLAIM_OFFSET]
            .iter()
            .any(|b| *b != 0)
    {
        return Err(Error::InvalidStateEncoding);
    }

    // Decode the packed byte layout into a strongly-typed Rust struct.
    Ok(StateData {
        seller_blake160: data[SELLER_BLAKE160_OFFSET..SELLER_BLAKE160_OFFSET + 20]
            .try_into()
            .unwrap(),
        buyer_blake160: data[BUYER_BLAKE160_OFFSET..BUYER_BLAKE160_OFFSET + 20]
            .try_into()
            .unwrap(),
        payout_lock_code_hash: data
            [PAYOUT_LOCK_CODE_HASH_OFFSET..PAYOUT_LOCK_CODE_HASH_OFFSET + 32]
            .try_into()
            .unwrap(),
        escrow_lock_code_hash: data
            [ESCROW_LOCK_CODE_HASH_OFFSET..ESCROW_LOCK_CODE_HASH_OFFSET + 32]
            .try_into()
            .unwrap(),
        udt_type_hash: data[UDT_TYPE_HASH_OFFSET..UDT_TYPE_HASH_OFFSET + 32]
            .try_into()
            .unwrap(),
        dispute_started: data[DISPUTE_STARTED_OFFSET] == 1,
        seller_claim_udt: read_u128_le(&data[SELLER_CLAIM_OFFSET..]),
    })
}

fn sum_linked_escrows(
    escrow_lock_code_hash: &[u8],
    udt_type_hash: &[u8],
    state_type_hash: &[u8],
    source: Source,
) -> Result<u128, Error> {
    // Sum all escrow cells that are linked to this state by exact lock code hash
    // plus the full companion state type hash stored in lock args and the
    // exact UDT type hash configured in state.
    let mut total = 0u128;
    let mut i = 0usize;

    loop {
        let lock = match load_cell_lock(i, source) {
            Ok(lock) => lock,
            Err(SysError::IndexOutOfBound) => break,
            Err(e) => return Err(e.into()),
        };

        if lock.code_hash().as_slice() == escrow_lock_code_hash
            && lock.args().raw_data().as_ref() == state_type_hash
        {
            // Linked escrow cells must use the exact configured UDT type hash.
            let Some(type_hash) = load_cell_type_hash(i, source)? else {
                continue;
            };
            if type_hash.as_slice() != udt_type_hash {
                continue;
            }
            let data = load_cell_data(i, source)?;
            if data.len() < 16 {
                return Err(Error::EscrowAccountingMismatch);
            }
            total += read_u128_le(data.as_ref());
        }

        i += 1;
    }

    Ok(total)
}

fn collect_payout_outputs(
    seller_blake160: &[u8; 20],
    buyer_blake160: &[u8; 20],
    payout_lock_code_hash: &[u8; 32],
    udt_type_hash: &[u8; 32],
) -> Result<(u128, u128), Error> {
    // Aggregate seller and buyer payout amounts from the transaction outputs.
    // We only count outputs that use the expected payout lock code hash and
    // whose 20-byte args match seller or buyer blake160.
    let mut seller_amt = 0u128;
    let mut buyer_amt = 0u128;
    let mut i = 0usize;

    loop {
        let lock = match load_cell_lock(i, Source::Output) {
            Ok(lock) => lock,
            Err(SysError::IndexOutOfBound) => break,
            Err(e) => return Err(e.into()),
        };

        // Ignore outputs that do not use the agreed payout lock code hash.
        if lock.code_hash().as_slice() == payout_lock_code_hash {
            let Some(type_hash) = load_cell_type_hash(i, Source::Output)? else {
                i += 1;
                continue;
            };
            if &type_hash != udt_type_hash {
                i += 1;
                continue;
            }
            let args = lock.args().raw_data();
            // We only recognize standard 20-byte lock-arg outputs here.
            if args.len() == 20 {
                let data = load_cell_data(i, Source::Output)?;
                // UDT-compatible amount is the first 16 bytes of cell data.
                let amount = if data.len() >= 16 {
                    read_u128_le(data.as_ref())
                } else {
                    0
                };

                // Route the amount into the correct bucket based on lock args.
                if args.as_ref() == seller_blake160.as_slice() {
                    seller_amt += amount;
                } else if args.as_ref() == buyer_blake160.as_slice() {
                    buyer_amt += amount;
                }
            }
        }

        i += 1;
    }

    Ok((seller_amt, buyer_amt))
}

fn verify_secp256k1(blake160: &[u8; 20], sig_bytes: &[u8], msg: &[u8; 32]) -> Result<(), Error> {
    use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};

    // Reject malformed signature lengths immediately.
    if sig_bytes.len() != SIG_LEN {
        return Err(Error::InvalidSignature);
    }

    // Recover the secp256k1 public key from the message prehash + signature.
    let recovery_id = RecoveryId::from_byte(sig_bytes[64] % 4).ok_or(Error::InvalidSignature)?;
    let sig = Signature::from_slice(&sig_bytes[..64]).map_err(|_| Error::InvalidSignature)?;
    let recovered = VerifyingKey::recover_from_prehash(msg, &sig, recovery_id)
        .map_err(|_| Error::SignatureMismatch)?;

    // Convert the recovered compressed pubkey into CKB's blake160 identity form.
    let encoded = recovered.to_encoded_point(true);
    let hash = blake2b_256(encoded.as_bytes());
    if &hash[..20] != blake160 {
        return Err(Error::SignatureMismatch);
    }

    Ok(())
}

fn hash_outpoint(input_buf: &[u8; 36], output_index: u32) -> [u8; 32] {
    // Channel id is derived from:
    // - the first input cell's previous outpoint (`tx_hash || index_LE4`)
    // - this state cell's actual output index (`output_index_LE4`)
    //
    // That keeps ids unique even when multiple fresh channel-state cells are
    // created in the same transaction.
    let mut payload = [0u8; 40];
    payload[..36].copy_from_slice(input_buf);
    payload[36..40].copy_from_slice(&output_index.to_le_bytes());
    blake2b_256(&payload)
}

fn blake2b_256(data: &[u8]) -> [u8; 32] {
    use blake2b_ref::Blake2bBuilder;

    // Build the canonical 32-byte CKB default hash.
    let mut out = [0u8; 32];
    let mut hasher = Blake2bBuilder::new(32)
        .personal(b"ckb-default-hash")
        .build();
    hasher.update(data);
    hasher.finalize(&mut out);
    out
}

fn read_u128_le(b: &[u8]) -> u128 {
    // Read little-endian `u128` values from packed channel state bytes.
    u128::from_le_bytes(b[..16].try_into().unwrap())
}
