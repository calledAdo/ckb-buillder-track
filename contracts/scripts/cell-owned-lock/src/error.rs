use ckb_std::error::SysError;
use core::convert::From;

/// Errors returned by the cell-owned lock.
///
/// This lock now has a narrow job:
/// - verify the linked channel cell is present in the transaction when escrow is spent
/// - do nothing else
///
/// Because of that, the error surface is much smaller than the state type
/// script's error surface.
#[repr(i8)]
#[derive(Debug, Clone, Copy)]
pub enum Error {
    // ── System errors forwarded from ckb-std ─────────────────────────────────
    IndexOutOfBound = 1,
    ItemMissing = 2,
    LengthNotEnough = 3,
    Encoding = 4,

    // ── Structure errors ──────────────────────────────────────────────────────
    /// Escrow lock args must be exactly one 32-byte linked-cell type hash.
    InvalidArgsLength = 10,
    /// No linked channel cell with the expected type hash was found.
    MissingCompanionCell = 11,
}

impl From<SysError> for Error {
    fn from(e: SysError) -> Self {
        // Preserve useful syscall failure categories while still returning the
        // compact i8 error codes CKB expects from scripts.
        match e {
            SysError::IndexOutOfBound => Self::IndexOutOfBound,
            SysError::ItemMissing => Self::ItemMissing,
            SysError::LengthNotEnough(_) => Self::LengthNotEnough,
            SysError::Encoding => Self::Encoding,
            _ => Self::Encoding,
        }
    }
}
