//! LendNerv Active Loan Vault Data (`vault_data.rs`).
//!
//! # On-chain encoding
//!
//! A vault cell is an xUDT cell of the collateral token (Asset A) protected
//! by the vault lock script.  Its `data` field is left **untouched** — it
//! contains only the standard 16-byte xUDT balance.  All loan-specific
//! metadata lives in the vault **lock script args**, preserving full
//! compatibility with any custom xUDT token that stores extension data
//! after its balance field.
//!
//! ## Vault lock args layout (113 bytes)
//!
//! ```text
//! Offset  Size  Field
//! ──────────────────────────────────────────────
//!  0      32    borrower_lock_hash  ([u8; 32])
//! 32      32    pool_type_hash      ([u8; 32])
//! 64      16    principal           (u128 LE)
//! 80      16    r_vault             (u128 LE)
//! 96       8    t_created           (u64  LE)
//! 104      8    t_exp               (u64  LE)
//! 112      1    is_frozen           (0 | 1)
//! ──────────────────────────────────────────────
//! Total: 113 bytes
//! ```
//!
//! The cell data field carries only the standard xUDT 16-byte LE u128
//! collateral amount, which callers read separately via `read_sudt_amount`.

// ─────────────────────────────────────────────────────────────────────────────
// Layout constants (byte offsets within the lock args)
// ─────────────────────────────────────────────────────────────────────────────

/// Total required length of the vault lock args.
pub const VAULT_ARGS_LEN: usize = 113;

// Offsets of the fixed header fields (not parsed by VaultData directly).
pub const OFF_BORROWER_LOCK_HASH: usize = 0; // [u8; 32]
pub const OFF_POOL_TYPE_HASH: usize = 32;      // [u8; 32]

// Offsets of the loan metadata fields parsed by `from_lock_args`.
const OFF_PRINCIPAL: usize = 64;  // u128 LE (16 bytes)
const OFF_R_VAULT:   usize = 80;  // u128 LE (16 bytes)
const OFF_T_CREATED: usize = 96;  // u64  LE  (8 bytes)
const OFF_T_EXP:     usize = 104; // u64  LE  (8 bytes)
const OFF_IS_FROZEN: usize = 112; //  u8      (1 byte)

// ─────────────────────────────────────────────────────────────────────────────
// VaultData
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime representation of a single active loan vault cell.
///
/// `collateral_amt` is read from the cell data (standard xUDT amount).
/// All other fields are read from the vault lock script args.
///
/// The cell can only be consumed (Repayment, Liquidation, or Freeze);
/// the loan fields are immutable except `is_frozen`, which flips 0→1
/// during a Freeze transaction.
#[derive(Clone)]
pub struct VaultData {
    /// Borrower's 32-byte lock hash used by vault ownership checks.
    pub borrower_lock_hash: [u8; 32],
    /// Pool type hash this vault is bound to.
    pub pool_type_hash: [u8; 32],
    /// Amount of Asset A (collateral token) locked in this cell.
    /// Sourced from cell data (standard xUDT amount at offset 0).
    pub collateral_amt: u128,
    /// Asset B principal borrowed from the pool.
    pub principal: u128,
    /// Frozen per-second interest contribution scaled by RATE_PRECISION.
    pub r_vault: u128,
    /// Unix timestamp (seconds) at which the vault was created.
    pub t_created: u64,
    /// Deadline timestamp (`t_created + origination_duration`).
    pub t_exp: u64,
    /// `0` = active, `1` = frozen (r_vault already removed from rate_net).
    pub is_frozen: u8,
}

impl VaultData {
    /// Parse a vault from its lock script args and the cell's xUDT amount.
    ///
    /// - `args`           — full lock script args (must be ≥ [`VAULT_ARGS_LEN`])
    /// - `collateral_amt` — xUDT balance pre-read from cell data
    ///
    /// Returns `None` if `args` is too short.
    pub fn from_lock_args(args: &[u8], collateral_amt: u128) -> Option<Self> {
        if args.len() < VAULT_ARGS_LEN {
            return None;
        }

        let mut borrower_lock_hash = [0u8; 32];
        borrower_lock_hash.copy_from_slice(&args[OFF_BORROWER_LOCK_HASH..OFF_BORROWER_LOCK_HASH + 32]);

        let mut pool_type_hash = [0u8; 32];
        pool_type_hash.copy_from_slice(&args[OFF_POOL_TYPE_HASH..OFF_POOL_TYPE_HASH + 32]);

        let mut principal_bytes = [0u8; 16];
        principal_bytes.copy_from_slice(&args[OFF_PRINCIPAL..OFF_PRINCIPAL + 16]);

        let mut r_vault_bytes = [0u8; 16];
        r_vault_bytes.copy_from_slice(&args[OFF_R_VAULT..OFF_R_VAULT + 16]);

        let mut t_created_bytes = [0u8; 8];
        t_created_bytes.copy_from_slice(&args[OFF_T_CREATED..OFF_T_CREATED + 8]);

        let mut t_exp_bytes = [0u8; 8];
        t_exp_bytes.copy_from_slice(&args[OFF_T_EXP..OFF_T_EXP + 8]);

        Some(VaultData {
            borrower_lock_hash,
            pool_type_hash,
            collateral_amt,
            principal:  u128::from_le_bytes(principal_bytes),
            r_vault:    u128::from_le_bytes(r_vault_bytes),
            t_created:  u64::from_le_bytes(t_created_bytes),
            t_exp:      u64::from_le_bytes(t_exp_bytes),
            is_frozen:  args[OFF_IS_FROZEN],
        })
    }

    /// Serialise the loan metadata fields into the 49-byte lock args extension
    /// (bytes [64..113]).
    ///
    /// The aggregator prepends `borrower_lock_hash` (32 bytes) and
    /// `pool_type_hash` (32 bytes) when constructing the full lock args for
    /// a vault output cell.
    pub fn to_args_extension(&self) -> alloc::vec::Vec<u8> {
        let mut buf = alloc::vec![0u8; VAULT_ARGS_LEN - 64]; // 49 bytes
        buf[0..16].copy_from_slice(&self.principal.to_le_bytes());
        buf[16..32].copy_from_slice(&self.r_vault.to_le_bytes());
        buf[32..40].copy_from_slice(&self.t_created.to_le_bytes());
        buf[40..48].copy_from_slice(&self.t_exp.to_le_bytes());
        buf[48] = self.is_frozen;
        buf
    }
}
