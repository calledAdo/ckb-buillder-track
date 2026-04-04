//! LendNerv Global Pool State Data (`pool_data.rs`)
//!
//! Technical Overview:
//! This module defines the `PoolData` struct, which is the decoded Rust representation
//! of the global state stored directly inside the `Liquidity_Pool_Cell`'s data field.
//!
//! It handles the safe translation between the binary data stored on the Nervos blockchain
//! (using the Molecule serialization standard to minimize gas/cycles) and the runtime Rust
//! structs used by the `pool_script` to execute the protocol's mathematical validations.
//!
//! This data structure encapsulates:
//! 1. Dynamic ledgers tracking active debt, idle liquidity, idle supply, and lazy sum of interest.
//! 2. Immutable governance rules determining risk controls (k_min, base_rate, max_duration).
//! 3. Type Hashes uniquely identifying the physical sUDT tokens governed by this specific pool.

use crate::lendnerv::{Byte32, PoolState, PoolStateBuilder, Uint128, Uint64};
use molecule::prelude::*;

// ─────────────────────────────────────────────────────────────────────────────
// PoolData: parsed view of the pool cell's data field using Molecule
// ─────────────────────────────────────────────────────────────────────────────

/// The runtime abstraction of the Liquidity Pool State.
/// Translates the strict byte array into native Rust `u128` and `u64` integers
/// to safely perform mathematical derivations in `math.rs`.
#[derive(Clone)]
pub struct PoolData {
    // ── Dynamic state (Variables updated by user actions) ──────────
    /// Total amount of Asset B (e.g. USDC) physically sitting in the pool cell.
    pub liquidity_available: u128,
    /// Total principal Asset B currently checked-out by active borrowers.
    pub total_debt: u128,
    /// The "lazy evaluation" ledger mapping theoretical, unrealized pool profit.
    pub interest_accrued: u128,
    /// The running summation of every single active vault's `r_vault` fixed rate per second.
    pub rate_net: u128, // scaled by RATE_PRECISION
    /// Global supply of LP tokens distributed to all lenders across the network.
    pub total_lp_supply: u128,
    /// The exact block timestamp (in seconds) the smart contract state was last modified.
    pub timestamp_last_update: u64,

    // ── Dynamic Risk Variables (The Self-Healing State) ──
    /// The active minimum collateral ratio. This moves up and down based on market health.
    pub k_min_current: u64,
    /// The exact block time the pool last took a loss/haircut.
    pub timestamp_last_haircut: u64,

    // ── Governance parameters (Variables frozen between authority events) ──
    /// The absolute lowest allowable collateral-to-loan token quantity ratio limit.
    pub k_min: u64,
    /// The target ratio where duration calculations max out for loan safety.
    pub k_target: u64,
    /// Absolute maximum number of seconds a borrower can lock a vault open.
    pub t_max: u64,
    /// The minimum global interest spread on pristine utilization bounds.
    pub base_rate: u64,
    /// The absolute maximum global interest limit applied when the pool hits 100% utilization.
    pub max_rate: u64,

    // ── Asset identifiers (Tying this pool state to physical token boundaries) ──
    /// The native Nervos type hash representing the loaned asset (e.g. USDC type script).
    pub asset_b_type_hash: [u8; 32],
    /// The native Nervos type hash representing the collateral asset (e.g. WCKB type script).
    pub asset_a_type_hash: [u8; 32],
    pub lp_token_type_hash: [u8; 32],
    /// The lock hash of the vault lock script for this pool, used to identify authentic Vault cells.
    pub vault_lock_code_hash: [u8; 32],
    /// The static cryptographic lock hash that permanently secures the pool's physical token capital.
    pub pool_lock_hash: [u8; 32],
}

impl PoolData {
    /// Deserializes the raw Cell Data payload using Molecule code-generations.
    ///
    /// The data structure uses tightly packed little-endian integers mapped over the layout
    /// generated in `lendnerv.rs`. This function bridges between `Uint128` raw bytes and Rust `u128`.
    ///
    /// Returns `None` if the cell data is corrupted, improperly formatted, or the wrong length.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        // Attempt to parse the byte slice into the Molecule-generated wrapper.
        // Returns an Err if the internal block offsets or schema constraints break.
        let pool_state = PoolState::from_slice(data).ok()?;

        // Perform safe slice copies to pull out the 32-byte hash identifiers.
        let mut b_hash = [0u8; 32];
        b_hash.copy_from_slice(pool_state.asset_b_type_hash().as_slice());

        let mut a_hash = [0u8; 32];
        a_hash.copy_from_slice(pool_state.asset_a_type_hash().as_slice());

        let mut lp_hash = [0u8; 32];
        lp_hash.copy_from_slice(pool_state.lp_token_type_hash().as_slice());

        let mut vault_hash = [0u8; 32];
        vault_hash.copy_from_slice(pool_state.vault_lock_code_hash().as_slice());

        let mut pool_lock = [0u8; 32];
        pool_lock.copy_from_slice(pool_state.pool_lock_hash().as_slice());

        // Construct the struct by explicitly mapping each Molecule byte wrapper
        // into standard little-endian native primitives for mathematical execution.
        Some(PoolData {
            liquidity_available: u128::from_le_bytes(pool_state.liquidity_available().into()),
            total_debt: u128::from_le_bytes(pool_state.total_debt().into()),
            interest_accrued: u128::from_le_bytes(pool_state.interest_accrued().into()),
            rate_net: u128::from_le_bytes(pool_state.rate_net().into()),
            total_lp_supply: u128::from_le_bytes(pool_state.total_lp_supply().into()),
            timestamp_last_update: u64::from_le_bytes(pool_state.timestamp_last_update().into()),
            k_min_current: u64::from_le_bytes(pool_state.k_min_current().into()),
            timestamp_last_haircut: u64::from_le_bytes(pool_state.timestamp_last_haircut().into()),
            // Extract the governance constraints.
            k_min: u64::from_le_bytes(pool_state.k_min().into()),
            k_target: u64::from_le_bytes(pool_state.k_target().into()),
            t_max: u64::from_le_bytes(pool_state.t_max().into()),
            base_rate: u64::from_le_bytes(pool_state.base_rate().into()),
            max_rate: u64::from_le_bytes(pool_state.max_rate().into()),
            // Map the token hashes validating the pool identity.
            asset_b_type_hash: b_hash,
            asset_a_type_hash: a_hash,
            lp_token_type_hash: lp_hash,
            vault_lock_code_hash: vault_hash,
            pool_lock_hash: pool_lock,
        })
    }

    /// Serializes the struct back into a dense binary byte array using Molecule.
    ///
    /// Executed whenever a state-transition transaction completes processing and needs
    /// to output the newly updated state variables sequentially directly back onto the blockchain ledger.
    pub fn to_bytes(&self) -> alloc::vec::Vec<u8> {
        // Utilize the generated Molecule builder to securely and sequentially pack every byte layer.
        let pool_state = PoolStateBuilder::default()
            // Map all `u128` ledgers natively through generic `Uint128` wrappers converting to little-endian bytes.
            .liquidity_available(Uint128::from(self.liquidity_available.to_le_bytes()))
            .total_debt(Uint128::from(self.total_debt.to_le_bytes()))
            .interest_accrued(Uint128::from(self.interest_accrued.to_le_bytes()))
            .rate_net(Uint128::from(self.rate_net.to_le_bytes()))
            .total_lp_supply(Uint128::from(self.total_lp_supply.to_le_bytes()))
            // Map the timestamps and governance controls down to `u64` constraints.
            .timestamp_last_update(Uint64::from(self.timestamp_last_update.to_le_bytes()))
            .k_min_current(Uint64::from(self.k_min_current.to_le_bytes()))
            .timestamp_last_haircut(Uint64::from(self.timestamp_last_haircut.to_le_bytes()))
            .k_min(Uint64::from(self.k_min.to_le_bytes()))
            .k_target(Uint64::from(self.k_target.to_le_bytes()))
            .t_max(Uint64::from(self.t_max.to_le_bytes()))
            .base_rate(Uint64::from(self.base_rate.to_le_bytes()))
            .max_rate(Uint64::from(self.max_rate.to_le_bytes()))
            // Map standard hash signatures.
            .asset_b_type_hash(Byte32::from(self.asset_b_type_hash))
            .asset_a_type_hash(Byte32::from(self.asset_a_type_hash))
            .lp_token_type_hash(Byte32::from(self.lp_token_type_hash))
            .vault_lock_code_hash(Byte32::from(self.vault_lock_code_hash))
            .pool_lock_hash(Byte32::from(self.pool_lock_hash))
            .build();

        // Export the finalized packed struct as a flat byte vector to become the Output Cell's direct Data field.
        pool_state.as_bytes().to_vec()
    }

    /// Validates absolute rigidity across all static governance parameters and token mappings.
    ///
    /// This prevents an aggregator from secretly editing the `k_min` parameter or changing the
    /// destination token type hashes during a standard deposit, borrow, or withdrawal transaction.
    /// Used securely inside the validator checks.
    pub fn params_unchanged(&self, other: &PoolData) -> bool {
        // Sequentially verify exact equality across the 8 immutable system keys.
        self.k_min == other.k_min
            && self.k_target == other.k_target
            && self.t_max == other.t_max
            && self.base_rate == other.base_rate
            && self.max_rate == other.max_rate
            && self.asset_b_type_hash == other.asset_b_type_hash
            && self.asset_a_type_hash == other.asset_a_type_hash
            && self.lp_token_type_hash == other.lp_token_type_hash
            && self.vault_lock_code_hash == other.vault_lock_code_hash
            && self.pool_lock_hash == other.pool_lock_hash
    }
}
