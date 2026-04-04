use ckb_std::error::SysError;
use core::convert::From;

/// Errors returned by the payment-channel type script.
///
/// We keep these codes grouped by concern:
/// - basic syscall forwarding
/// - structural validation
/// - witness/signature validation
/// - state-transition validation
///
/// That makes it easier to reason about *where* a failing transaction went
/// wrong when debugging from the host side.
#[repr(i8)]
#[derive(Debug, Clone, Copy)]
pub enum Error {
    // ── System errors forwarded from ckb-std ─────────────────────────────────
    IndexOutOfBound = 1,
    ItemMissing = 2,
    LengthNotEnough = 3,
    Encoding = 4,

    // ── Structure errors ──────────────────────────────────────────────────────
    /// State type args must be exactly one 32-byte channel id.
    InvalidArgsLength = 10,
    /// State cell data must be exactly STATE_DATA_LEN bytes.
    InvalidStateDataLength = 11,
    /// The state script only supports 0->1, 1->1, or 1->0 transitions.
    InvalidGroupShape = 12,
    /// Channel id args must match blake2b(first_input_outpoint || output_index) at creation.
    InvalidChannelId = 13,
    /// A fresh state cell must start with zeroed mutable dispute fields.
    InitialStateMustBeOpen = 14,
    /// Immutable identity fields changed across an update.
    IdentityImmutable = 15,
    /// Linked escrow accounting derived from the transaction is invalid.
    EscrowAccountingMismatch = 16,
    /// State data uses an invalid dispute flag or non-zero reserved bytes.
    InvalidStateEncoding = 17,

    // ── Witness / signature errors ───────────────────────────────────────────
    /// WitnessArgs.lock is missing for the first group input.
    WitnessLockMissing = 20,
    /// Witness length does not match the mode being attempted.
    InvalidWitness = 21,
    /// Signature bytes are malformed.
    InvalidSignature = 22,
    /// Signature recovered public key does not match the expected blake160.
    SignatureMismatch = 23,

    // ── State transition errors ───────────────────────────────────────────────
    /// Cooperative close is not allowed once the channel entered dispute.
    AlreadyInDispute = 30,
    /// A new disputed state must move to a strictly higher cumulative seller claim.
    TicketNotHigherThanCurrent = 31,
    /// Output state data does not match the expected next state.
    InvalidStateTransition = 32,
    /// The payout input must use a valid relative timestamp `since` of at least 48 hours.
    InvalidPayoutSince = 33,
}

impl From<SysError> for Error {
    fn from(e: SysError) -> Self {
        // Map low-level ckb-std syscall errors into our compact script-level
        // error codes so callers get stable and intentional failures.
        match e {
            SysError::IndexOutOfBound => Self::IndexOutOfBound,
            SysError::ItemMissing => Self::ItemMissing,
            SysError::LengthNotEnough(_) => Self::LengthNotEnough,
            SysError::Encoding => Self::Encoding,
            _ => Self::Encoding,
        }
    }
}
