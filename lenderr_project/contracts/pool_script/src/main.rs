//! LendNerv Batched Pool Type Script Prototype (`pool_script_extended.rs`)
//!
//! Technical Overview (V2 Prototype):
//! This script explores "Uniform Batching"—allowing aggregators to process multiple Borrows,
//! Repays, or Liquidations simultaneously in a single transaction constraint rather than exactly 1:1.
//!
//! It achieves this by extracting all Vaults found in the transaction using iterator aggregation
//! and mathematically proving that the Total Net Flow of the global `Liquidity_Pool_Cell` correctly
//! accounts for the sum of all individual intent constraints.

#![no_std]
#![cfg_attr(not(test), no_main)]

#[cfg(test)]
extern crate alloc;

#[cfg(not(test))]
use ckb_std::default_alloc;

#[cfg(not(test))]
ckb_std::entry!(program_entry);
#[cfg(not(test))]
default_alloc!();

use alloc::vec::Vec;
use blake2b_ref::Blake2bBuilder;
use ckb_std::{
    ckb_constants::Source,
    ckb_types::{bytes::Bytes, prelude::*},
    high_level::{
        load_cell_data, load_cell_lock, load_cell_lock_hash, load_cell_type_hash, load_header,
        load_input, load_script, load_script_hash, QueryIter,
    },
    syscalls::SysError,
};

use lenderr_common::{
    errors::*,
    math::{
        annual_rate_bps, asset_b_for_burn, compute_duration, compute_r_vault, exchange_rate,
        interest_adjustment, lp_tokens_for_deposit, utilization, vault_interest_owed,
        RATE_PRECISION,
    },
    pool_data::PoolData,
    utils::read_sudt_amount,
    vault_data::VaultData,
};

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn program_entry() -> i8 {
    // ── Detect Creation vs Update ────────────────────────────────────────────
    // On first creation the pool cell exists only in GroupOutput, not GroupInput.
    // We detect this by attempting to load the first GroupInput; failure means creation.
    let is_creation = load_cell_data(0, Source::GroupInput).is_err();
    if is_creation {
        return validate_creation();
    }

    let old_data = match load_cell_data(0, Source::GroupInput) {
        Ok(d) => d,
        Err(_) => return ERROR_SYSCALL,
    };

    let new_data = match load_cell_data(0, Source::GroupOutput) {
        Ok(d) => d,
        Err(_) => return ERROR_SYSCALL,
    };

    let old = match PoolData::from_bytes(&old_data) {
        Some(p) => p,
        None => return ERROR_POOL_DATA_MALFORMED,
    };
    let new = match PoolData::from_bytes(&new_data) {
        Some(p) => p,
        None => return ERROR_POOL_DATA_MALFORMED,
    };

    if !old.params_unchanged(&new) {
        return ERROR_PARAMS_MUTATED;
    }

    let t_now_onchain = match load_header(0, Source::HeaderDep) {
        Ok(h) => h.raw().timestamp().unpack() / 1000,
        Err(_) => return ERROR_SYSCALL,
    };

    let t_now = new.timestamp_last_update;

    // Time validation: aggregator's declared t_now must be within 300 s of the
    // on-chain header timestamp
    // (propagation delay + aggregator build time) could revert valid transactions.
    // 5 minutes gives ample headroom without meaningfully affecting curve precision.
    let time_delta = if t_now > t_now_onchain {
        t_now - t_now_onchain
    } else {
        t_now_onchain - t_now
    };
    if time_delta > 300 {
        return ERROR_POOL_DATA_MALFORMED;
    }
    if t_now <= old.timestamp_last_update {
        return ERROR_POOL_DATA_MALFORMED;
    }

    // ── Route the Batched Action ──
    match detect_action(&old, &new, t_now) {
        Action::BatchedDeposit(deposit_amount) => {
            validate_deposit(&old, &new, deposit_amount, t_now)
        }
        Action::BatchedWithdraw(burn_amount) => validate_withdraw(&old, &new, burn_amount, t_now),
        Action::Borrow(vault) => validate_borrow(&old, &new, vault, t_now),
        Action::BatchedRepay(vaults) => validate_batched_repay(&old, &new, vaults, t_now),
        Action::BatchedLiquidate(vaults) => validate_batched_liquidate(&old, &new, vaults, t_now),
        Action::BatchedFreeze(vaults) => validate_freeze(&old, &new, vaults, t_now),
        Action::Unknown => ERROR_INVALID_ACTION,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Action Detection (Batched)
// ─────────────────────────────────────────────────────────────────────────────

enum Action {
    BatchedDeposit(u128),
    BatchedWithdraw(u128),
    Borrow(VaultData),
    BatchedRepay(Vec<VaultData>),
    BatchedLiquidate(Vec<VaultData>),
    /// Expired vaults being frozen in-place: r_vault removed from rate_net,
    /// phantom overcharge erased, is_frozen flipped 0→1.  Vault cell persists.
    BatchedFreeze(Vec<VaultData>),
    Unknown,
}

fn detect_action(old: &PoolData, new: &PoolData, t_now: u64) -> Action {
    let vaults_in: Vec<VaultData> = find_all_vaults(old, Source::Input)
        .into_iter()
        .map(|(_, vd)| vd)
        .collect();
    let vaults_out: Vec<VaultData> = find_all_vaults(old, Source::Output)
        .into_iter()
        .map(|(_, vd)| vd)
        .collect();

    // If Vaults are being created (Outputs > Inputs) -> Borrow Batch
    // We strictly limit Borrows to 1-per-tx to prevent utilization path-dependency slippage.
    if vaults_out.len() == 1 && vaults_in.is_empty() && new.total_debt > old.total_debt {
        return Action::Borrow(vaults_out.into_iter().next().unwrap());
    }

    // If Vaults are being consumed (Inputs > Outputs) -> Repay or Liquidate Batch
    if vaults_in.len() > 0 && vaults_out.is_empty() {
        // Enforce Uniform Batching: All vaults in the batch must be either cleanly expired or completely active.
        let is_liquidation_batch = vaults_in.iter().all(|v| t_now >= v.t_exp);
        let is_repay_batch = vaults_in.iter().all(|v| t_now < v.t_exp);

        if is_liquidation_batch {
            return Action::BatchedLiquidate(vaults_in);
        } else if is_repay_batch {
            return Action::BatchedRepay(vaults_in);
        } else {
            // Mixed batches (some expired, some not) are rejected in this uniform prototype.
            return Action::Unknown;
        }
    }

    // ── Freeze Detection ──────────────────────────────────────────────────────
    // A Freeze TX keeps the same number of vault cells in input AND output.
    // Every input vault must be active (is_frozen == 0) and expired (t_exp <= t_now),
    // proving this is truly a freeze and not a disguised re-creation.
    if vaults_in.len() > 0 && vaults_in.len() == vaults_out.len() {
        let all_unfrozen_and_expired = vaults_in
            .iter()
            .all(|v| v.is_frozen == 0 && t_now >= v.t_exp);
        if all_unfrozen_and_expired {
            return Action::BatchedFreeze(vaults_in);
        }
    }

    // ── Pure Accounting Actions (Batched inherently) ──
    if new.total_lp_supply > old.total_lp_supply {
        let deposit_amount = new
            .liquidity_available
            .saturating_sub(old.liquidity_available);
        return Action::BatchedDeposit(deposit_amount);
    } else if new.total_lp_supply < old.total_lp_supply {
        let burn_amount = old.total_lp_supply.saturating_sub(new.total_lp_supply);
        return Action::BatchedWithdraw(burn_amount);
    }

    Action::Unknown
}

// ─────────────────────────────────────────────────────────────────────────────
// Net Flow Validation Rules (Batched)
// ─────────────────────────────────────────────────────────────────────────────

fn validate_borrow(old: &PoolData, new: &PoolData, vault: VaultData, t_now: u64) -> i8 {
    // Step 0: Apply Algorithmic Downward Decay (Peace Time Healing)
    let healed_old = apply_decay(old, t_now);

    // Step 1: Run Lazy Evaluation immediately shifting chronometric state into reality.
    let adj = interest_adjustment(
        healed_old.rate_net,
        healed_old.timestamp_last_update,
        t_now,
        0,
        0,
    );
    let base_interest = apply_adj(healed_old.interest_accrued, adj);

    // utilization should probably be based on new total debt and new liquidity available
    let u = utilization(healed_old.total_debt, healed_old.liquidity_available);
    let rate_bps = annual_rate_bps(u, healed_old.base_rate, healed_old.max_rate);

    let expected_r_vault = compute_r_vault(vault.principal, rate_bps);
    let max_interest = expected_r_vault * (healed_old.t_max as u128) / RATE_PRECISION;
    let max_debt = vault.principal + max_interest;

    let expected_duration = match compute_duration(
        vault.collateral_amt,
        max_debt,
        healed_old.k_min_current,
        healed_old.k_target,
        healed_old.t_max,
        u,
    ) {
        Some(d) => d,
        None => return ERROR_UNDERCOLLATERALIZED,
    };

    if expected_r_vault != vault.r_vault || vault.t_exp != vault.t_created + expected_duration {
        return ERROR_MATH_OVERFLOW;
    }

    // Vault creation timestamp must be pinned to the current block.
    // Prevents aggregator from back-dating t_created, which would silently
    // shorten t_exp relative to what the duration formula actually computed.
    if vault.t_created != t_now {
        return ERROR_POOL_DATA_MALFORMED;
    }

    // A freshly created vault must never be pre-marked as frozen.
    // is_frozen is set only by the subsequent freeze/liquidation flow.
    if vault.is_frozen != 0 {
        return ERROR_POOL_DATA_MALFORMED;
    }

    // ── Physical Asset Hard-Verification ──
    // Prove the aggregator didn't secretly drain more than the allowed loan principal.
    let pool_lock = old.pool_lock_hash;

    let pool_asset_b_in = compute_sudt_total(Source::Input, &old.asset_b_type_hash, &pool_lock);
    let pool_asset_b_out = compute_sudt_total(Source::Output, &old.asset_b_type_hash, &pool_lock);

    if pool_asset_b_out < pool_asset_b_in.saturating_sub(vault.principal) {
        return ERROR_DEBT_ACCOUNTING;
    }

    if new.liquidity_available != old.liquidity_available - vault.principal {
        return ERROR_DEBT_ACCOUNTING;
    }
    if new.total_debt != old.total_debt + vault.principal {
        return ERROR_DEBT_ACCOUNTING;
    }
    // Lazy evaluations synced perfectly.
    if new.interest_accrued != base_interest {
        return ERROR_INTEREST_ACCOUNTING;
    }
    // Update the system-wide accumulation tracking metric rigidly.
    if new.rate_net != old.rate_net + vault.r_vault {
        return ERROR_RATE_NET_MISMATCH;
    }
    // Synchronize epoch records securely.
    if new.timestamp_last_update != t_now {
        return ERROR_POOL_DATA_MALFORMED;
    }
    // Verify Downward Decay healing correctly applied natively across structural bounds safely measuring state transition.
    if new.k_min_current != healed_old.k_min_current
        || new.timestamp_last_haircut != healed_old.timestamp_last_haircut
    {
        return ERROR_POOL_DATA_MALFORMED;
    }

    // ── LP Supply Guard (Borrow must not mint or burn LP tokens) ─────────────
    // Owner mode is active whenever the pool cell is consumed, which is always.
    // Even though xUDT's user-mode conserves Σout ≤ Σin, owner mode bypasses
    // that gate.  pool_script must therefore close the gap itself.
    check_lp_unchanged(old, new)
}

fn validate_deposit(old: &PoolData, new: &PoolData, amt: u128, t_now: u64) -> i8 {
    // We STRICTLY PROJECT the base interest logically to find the truthful real-world exchange rate
    // for this exact block, ensuring LPs don't dilute value by depositing right before an accrual tick.
    let expected_adj = interest_adjustment(old.rate_net, old.timestamp_last_update, t_now, 0, 0);
    let projected_interest = apply_adj(old.interest_accrued, expected_adj);

    let rate = exchange_rate(
        old.liquidity_available,
        old.total_debt,
        projected_interest,
        old.total_lp_supply,
    );
    let expected_lp = lp_tokens_for_deposit(amt, rate);

    // ── Physical Asset Hard-Verification ──
    let pool_lock = old.pool_lock_hash.clone();
    let pool_asset_b_in = compute_sudt_total(Source::Input, &old.asset_b_type_hash, &pool_lock);
    let pool_asset_b_out = compute_sudt_total(Source::Output, &old.asset_b_type_hash, &pool_lock);

    if pool_asset_b_out < pool_asset_b_in.saturating_add(amt) {
        return ERROR_DEBT_ACCOUNTING;
    }

    // ── LP Supply Guard (Deposit must mint exactly expected_lp tokens) ────────
    // Guard 1: pool-state bookkeeping must reflect the formula-derived mint.
    if new.total_lp_supply != old.total_lp_supply + expected_lp {
        return ERROR_LP_MINT_INVALID;
    }
    // Guard 2: scan the actual LP xUDT cells — the physical net creation
    // must match the formula.  Together these two guards create defence-in-depth.
    {
        let lp_in = sum_lp_xudt(&old.lp_token_type_hash, Source::Input);
        let lp_out = sum_lp_xudt(&old.lp_token_type_hash, Source::Output);
        // lp_out - lp_in == expected_lp  (guarded against underflow)
        if lp_out < lp_in || lp_out - lp_in != expected_lp {
            return ERROR_LP_MINT_INVALID;
        }
    }

    if new.liquidity_available != old.liquidity_available + amt {
        return ERROR_DEBT_ACCOUNTING;
    }

    // Deposits advance the timestamp to t_now but do NOT commit the projected interest —
    // interest_accrued stays unchanged (the exchange rate was merely projected for LP math).
    // rate_net also stays unchanged because no vault is opened or closed.
    if new.interest_accrued != old.interest_accrued {
        return ERROR_INTEREST_ACCOUNTING;
    }
    if new.timestamp_last_update != t_now {
        return ERROR_POOL_DATA_MALFORMED;
    }

    0
}

fn validate_withdraw(old: &PoolData, new: &PoolData, burn_amount: u128, t_now: u64) -> i8 {
    let expected_adj = interest_adjustment(old.rate_net, old.timestamp_last_update, t_now, 0, 0);
    let projected_interest = apply_adj(old.interest_accrued, expected_adj);

    let rate = exchange_rate(
        old.liquidity_available,
        old.total_debt,
        projected_interest,
        old.total_lp_supply,
    );
    let expected_asset_b = asset_b_for_burn(burn_amount, rate);

    // ── Physical Asset Hard-Verification ──
    let pool_lock = old.pool_lock_hash.clone();
    let pool_asset_b_in = compute_sudt_total(Source::Input, &old.asset_b_type_hash, &pool_lock);
    let pool_asset_b_out = compute_sudt_total(Source::Output, &old.asset_b_type_hash, &pool_lock);

    if pool_asset_b_out < pool_asset_b_in.saturating_sub(expected_asset_b) {
        return ERROR_DEBT_ACCOUNTING;
    }

    // ── LP Supply Guard (Withdraw must burn exactly burn_amount tokens) ───────
    // Guard 1: pool-state bookkeeping must reflect the burn.
    if new.total_lp_supply != old.total_lp_supply - burn_amount {
        return ERROR_LP_MINT_INVALID;
    }
    // Guard 2: scan physical LP xUDT cells — net destruction must match.
    {
        let lp_in = sum_lp_xudt(&old.lp_token_type_hash, Source::Input);
        let lp_out = sum_lp_xudt(&old.lp_token_type_hash, Source::Output);
        // lp_in - lp_out == burn_amount  (guarded against underflow)
        if lp_in < lp_out || lp_in - lp_out != burn_amount {
            return ERROR_LP_MINT_INVALID;
        }
    }

    if new.liquidity_available != old.liquidity_available.saturating_sub(expected_asset_b) {
        return ERROR_DEBT_ACCOUNTING;
    }

    // Do not lock hard checkpoints on passive exits, strictly same principle as passive deposits.
    if new.interest_accrued != old.interest_accrued {
        return ERROR_INTEREST_ACCOUNTING;
    }
    if new.timestamp_last_update != old.timestamp_last_update {
        return ERROR_POOL_DATA_MALFORMED;
    }

    0
}
fn validate_batched_repay(
    old: &PoolData,
    new: &PoolData,
    vaults: Vec<VaultData>,
    t_now: u64,
) -> i8 {
    let mut total_principal_cleared = 0;
    let mut total_r_vault_cleared = 0;
    let mut total_interest_owed = 0;

    // 1. Accumulate specific mathematical bounds natively for every closed active vault.
    for vault in &vaults {
        total_principal_cleared += vault.principal;

        total_interest_owed += vault_interest_owed(vault.r_vault, vault.t_created, t_now);
        // We accumulate the r_vaults to deduct from the global rate_net securely.
        total_r_vault_cleared += vault.r_vault;
    }

    let total_repayment = total_principal_cleared + total_interest_owed;

    // Step 1: Engage Lazy Evaluation syncing the full global state first seamlessly considering the specific vaults closing.
    // Notice: We pass 0 for deductive_r explicitly here because we want to lazily accrue the global state up to exactly `t_now`.
    let adj = interest_adjustment(old.rate_net, old.timestamp_last_update, t_now, 0, 0);
    let base_interest = apply_adj(old.interest_accrued, adj);

    // Step 2: Extract the realized profit safely inherently scaling the `interest_accrued` ledger down.
    let new_interest = base_interest.saturating_sub(total_interest_owed);

    // ── Assert Exact System State Accounting Matches the Global Deltas ──
    if new.liquidity_available != old.liquidity_available + total_repayment {
        return ERROR_DEBT_ACCOUNTING;
    }
    if new.total_debt != old.total_debt - total_principal_cleared {
        return ERROR_DEBT_ACCOUNTING;
    }
    if new.interest_accrued != new_interest {
        return ERROR_INTEREST_ACCOUNTING;
    }
    // The underlying accumulator cleanly removes all these permanently fixed `r_vault` bounds simultaneously.
    if new.rate_net != old.rate_net - total_r_vault_cleared {
        return ERROR_RATE_NET_MISMATCH;
    }
    if new.timestamp_last_update != t_now {
        return ERROR_POOL_DATA_MALFORMED;
    }

    // ── Physical Asset Hard-Verification ──────────────────────────────────────────────
    // The borrower claims to have paid off the mathematically correct sum, checking to prove it explicitly arrived securely.
    // Instead of computing the pool's lock dynamically, we use the hardcoded immutable reference in the state pool data.
    let pool_lock = old.pool_lock_hash.clone();
    let pool_asset_b_in = compute_sudt_total(Source::Input, &old.asset_b_type_hash, &pool_lock);
    let pool_asset_b_out = compute_sudt_total(Source::Output, &old.asset_b_type_hash, &pool_lock);

    // Definitively verify that the transaction successfully increased the total native Token B held uniquely by the pool
    if pool_asset_b_out < pool_asset_b_in + total_repayment {
        return ERROR_DEBT_ACCOUNTING;
    }

    // ── LP Supply Guard (Repay must not mint or burn LP tokens) ──────────────
    check_lp_unchanged(old, new)
}
fn validate_batched_liquidate(
    old: &PoolData,
    new: &PoolData,
    vaults: Vec<VaultData>,
    t_now: u64,
) -> i8 {
    let mut total_principal_cleared = 0;
    let mut total_r_vault_cleared = 0;
    let mut total_interest_owed = 0;
    let mut total_phantom_interest = 0;
    let mut total_repayment_required = 0;

    // 1. Accumulate specific mathematical bounds natively for every closed defaulted vault.
    for vault in &vaults {
        total_principal_cleared += vault.principal;

        // Interest is capped at exactly `t_exp`
        let interest_owed = vault_interest_owed(vault.r_vault, vault.t_created, vault.t_exp);
        total_interest_owed += interest_owed;

        // Identify overcharge bounds specifically for this vault (Phantom Interest Clawback).
        // Since `rate_net` was universally tracking it, if it died at t_exp, it silently racked up phantom interest.
        //
        // IMPORTANT: always start from vault.t_exp, NOT max(t_exp, timestamp_last_update).
        // `old.interest_accrued` already contains r_vault contributions from t_exp →
        // old.timestamp_last_update if the vault expired before the last pool update.
        // Using `max` would leave that earlier phantom slice un-erased.
        // Total phantom == r_vault × (t_now − t_exp), split across both halves.
        if vault.is_frozen == 0 {
            total_phantom_interest +=
                vault.r_vault * (t_now - vault.t_exp) as u128 / RATE_PRECISION;
        }

        // Collect the r_vaults to deduct from the global rate_net securely (unless already frozen).
        let deduction = if vault.is_frozen == 1 {
            0
        } else {
            vault.r_vault
        };
        total_r_vault_cleared += deduction;

        // ── Calculate Dutch Auction Price specifically for this Vault ──
        let base_debt = vault.principal.saturating_add(interest_owed);
        let starting_premium = base_debt / 20; // 5% premium
        let auction_start_price = base_debt.saturating_add(starting_premium);

        // Time decay from exponent limit
        let time_passed = t_now.saturating_sub(vault.t_exp) as u128;
        let scaling_factor = RATE_PRECISION.saturating_add(time_passed.saturating_mul(100)); // DECAY_RATE_PER_SEC = 100

        let required_repayment = auction_start_price
            .checked_mul(RATE_PRECISION)
            .unwrap_or(u128::MAX)
            / scaling_factor;

        // Add to the massive unified repayment requirement.
        total_repayment_required += required_repayment;
    }

    // Determine the actual total underlying debt map specifically.
    let total_base_debt = total_principal_cleared + total_interest_owed;

    // What exactly did the aggregator physically push into the global pool?
    let pool_lock = old.pool_lock_hash.clone();
    let pool_asset_b_in = compute_sudt_total(Source::Input, &old.asset_b_type_hash, &pool_lock);
    let pool_asset_b_out = compute_sudt_total(Source::Output, &old.asset_b_type_hash, &pool_lock);

    let physical_bid = pool_asset_b_out.saturating_sub(pool_asset_b_in);

    if physical_bid < total_repayment_required {
        return ERROR_BID_TOO_LOW;
    }

    // Step 1: Engage Lazy Evaluation syncing the full global state first seamlessly considering the specific vaults closing.
    let adj = interest_adjustment(
        old.rate_net,
        old.timestamp_last_update,
        t_now,
        0, // Deductions grouped externally to base tracking
        0,
    );
    let base_interest = apply_adj(old.interest_accrued, adj);

    // To prevent the theoretical tracking vectors breaking if someone overpays the principal (due to premium),
    // the system explicitly recognizes physical debt completion capped safely natively.
    let actual_repay = physical_bid.min(total_base_debt);
    // Realize accrued profit and Clawback the phantom interest intrinsically across all expired vaults
    let new_interest = base_interest
        .saturating_sub(total_interest_owed)
        .saturating_sub(total_phantom_interest);

    // Step 2: The Upward Ratchet (Taking Damage & Securing the Protocol)
    let haircut = actual_repay < total_base_debt;

    // Set the state dynamically routing default mapping safely.
    let final_k_min: u64;
    let final_timestamp: u64;

    if haircut {
        // ── Upward Ratchet: Immune Response ──────────────────────────────────────
        //
        // STEP 1 — Global Severity: measure the loss against the entire protocol TVL,
        // not just the defaulted loan. Prevents a tiny loan from triggering a false panic.
        //
        //   Haircut_Amount = total_base_debt − actual_repay
        //   Global_TVL     = liquidity_available + total_debt + interest_accrued
        //   Severity       = Haircut_Amount / Global_TVL          (fraction in [0, 1])
        //
        let haircut_amount = total_base_debt.saturating_sub(actual_repay);
        let global_tvl = old
            .liquidity_available
            .saturating_add(old.total_debt)
            .saturating_add(old.interest_accrued);
        // Severity in fixed-point (scaled by 10_000 for integer math).
        let severity_bps = if global_tvl > 0 {
            haircut_amount.saturating_mul(10_000) / global_tvl
        } else {
            10_000 // 100% severity if pool is somehow empty
        };

        // STEP 2 — Time-Weighted Panic (Volatility Clustering):
        // If a second haircut hits while the healing clock is still running, the
        // multiplier amplifies the penalty — the protocol detects a live crash.
        //
        //   Elapsed          = t_now − timestamp_last_haircut
        //   Time_Multiplier  = 1.0 + max(0, (HEALING_PERIOD − Elapsed) / HEALING_PERIOD)
        //
        // At t=0 (instant repeat hit): multiplier = 2.0  → 2× penalty.
        // At t=HEALING_PERIOD/2:       multiplier = 1.5  → 1.5× penalty.
        // At t≥HEALING_PERIOD:         multiplier = 1.0  → no amplification.
        //
        let elapsed_since_last = t_now.saturating_sub(old.timestamp_last_haircut);
        // time_multiplier in fixed-point × 10_000 (1.0 = 10_000, 2.0 = 20_000).
        let time_multiplier_bps = if elapsed_since_last < HEALING_PERIOD {
            let remaining = HEALING_PERIOD - elapsed_since_last;
            10_000u128 + (remaining as u128 * 10_000 / HEALING_PERIOD as u128)
        } else {
            10_000u128 // 1.0 — no cluster amplification
        };

        // STEP 3 — Final Penalty with Death-Spiral Cap:
        //   Penalty    = k_min_current × Severity × Time_Multiplier
        //   k_min_new  = min(K_MAX_CAP, k_min_current + Penalty)
        //
        let k_max_cap = old.k_target.saturating_mul(K_MAX_SCALE);
        // Multiply all numerators FIRST, then divide by the combined denominator once.
        // Doing it left-to-right (÷10_000 before ×time_multiplier) would truncate to 0
        // for small severity values (e.g. 0.5% loss → severity_bps=50 → 150*50=7500 ÷10_000=0).
        // Combined denominator: 10_000 × 10_000 = 100_000_000.
        let penalty =
            (old.k_min_current as u128 * severity_bps * time_multiplier_bps / 100_000_000) as u64;
        final_k_min = old.k_min_current.saturating_add(penalty).min(k_max_cap);
        final_timestamp = t_now;
    } else {
        // If it was a perfectly healthy liquidation, apply normal downward decay.
        let healed = apply_decay(old, t_now);
        final_k_min = healed.k_min_current;
        final_timestamp = healed.timestamp_last_haircut;
    }

    // ── Assert Exact System State Accounting Matches the Global Deltas ──
    if new.liquidity_available != old.liquidity_available + physical_bid {
        return ERROR_DEBT_ACCOUNTING;
    }
    if new.total_debt != old.total_debt - total_principal_cleared {
        return ERROR_DEBT_ACCOUNTING;
    }

    if new.interest_accrued != new_interest {
        return ERROR_INTEREST_ACCOUNTING;
    }
    if new.rate_net != old.rate_net - total_r_vault_cleared {
        return ERROR_RATE_NET_MISMATCH;
    }
    if new.timestamp_last_update != t_now {
        return ERROR_POOL_DATA_MALFORMED;
    }
    if new.k_min_current != final_k_min || new.timestamp_last_haircut != final_timestamp {
        return ERROR_POOL_DATA_MALFORMED;
    }

    // ── LP Supply Guard (Liquidate must not mint or burn LP tokens) ───────────
    check_lp_unchanged(old, new)
}

// ─────────────────────────────────────────────────────────────────────────────
// Vault Freeze Validator
// ─────────────────────────────────────────────────────────────────────────────

/// Validates a "Freeze" transaction: an expired-but-not-yet-liquidated vault
/// has its rate contribution removed from the pool and its phantom overcharge
/// erased, while the vault cell itself is kept alive with `is_frozen = 1`.
///
/// ## What changes in the pool
///
/// | Field             | Direction         |
/// |-------------------|-------------------|
/// | `rate_net`        | ↓ by Σ r_vault    |
/// | `interest_accrued`| ↓ by Σ phantom interest since t_exp |
/// | `total_debt`      | unchanged          |
/// | `liquidity_available` | unchanged      |
/// | `k_min_current`   | may decay (peace time healing applied) |
///
/// ## What the vault output must look like
///
/// Every field is identical to its input counterpart EXCEPT `is_frozen`, which
/// must be flipped from `0` to `1`.  This is verified by re-scanning the
/// output vault cells and cross-checking field by field.
fn validate_freeze(old: &PoolData, new: &PoolData, vaults: Vec<VaultData>, t_now: u64) -> i8 {
    // Accumulate the totals we expect to subtract from the pool.
    let mut total_r_vault_removed: u128 = 0;
    let mut total_phantom_interest: u128 = 0;

    for vault in &vaults {
        // Precondition (also enforced by detect_action, but re-checked here
        // for defence-in-depth): vault must be expired and not already frozen.
        if vault.is_frozen != 0 {
            return ERROR_VAULT_DATA_MALFORMED;
        }
        if t_now < vault.t_exp {
            return ERROR_VAULT_PREMATURE_FREEZE;
        }

        // r_vault contribution to be removed from rate_net.
        total_r_vault_removed += vault.r_vault;

        // Phantom interest = r_vault accrued since t_exp (the portion the pool
        // never "earned" because the loan was already past its deadline).
        //
        // Always start from vault.t_exp — NOT max(t_exp, timestamp_last_update).
        // old.interest_accrued already contains r_vault contributions from
        // t_exp → old.timestamp_last_update if the vault expired before the
        // last pool update.  Missing that slice would leave phantom interest
        // permanently embedded in the pool's accrued profits.
        if t_now > vault.t_exp {
            total_phantom_interest +=
                vault.r_vault * (t_now - vault.t_exp) as u128 / RATE_PRECISION;
        }
    }

    // ── Recompute expected new interest_accrued ──────────────────────────────
    // First, lazily advance interest to t_now using the OLD rate_net (before
    // removing the frozen vaults), then subtract the phantom overcharge.
    let adj = interest_adjustment(old.rate_net, old.timestamp_last_update, t_now, 0, 0);
    let base_interest = apply_adj(old.interest_accrued, adj);
    let expected_interest = base_interest.saturating_sub(total_phantom_interest);

    // ── Apply peace-time decay (same as borrow / healthy-liquidation path) ──
    let healed = apply_decay(old, t_now);

    // ── Verify output vault cells (O(N) zip — aggregator MUST preserve order) ──
    // Requiring output vaults to mirror input vaults positionally reduces the
    // match from O(N²) nested iterations to a single O(N) zip, cutting CKB-VM
    // cycle cost for large freeze batches by up to 50×.
    //
    // The index is needed to load each output cell's lock for the code_hash
    // continuity assertion below.
    let vaults_out = find_all_vaults(old, Source::Output);

    if vaults_out.len() != vaults.len() {
        return ERROR_POOL_DATA_MALFORMED;
    }

    for (vin, (out_idx, vout)) in vaults.iter().zip(vaults_out.iter()) {
        // ── Data continuity: every field identical except is_frozen ──────────
        if vout.collateral_amt != vin.collateral_amt
            || vout.principal != vin.principal
            || vout.r_vault != vin.r_vault
            || vout.t_created != vin.t_created
            || vout.t_exp != vin.t_exp
            || vout.is_frozen != 1
        // ← the ONLY field allowed to change
        {
            return ERROR_VAULT_DATA_MALFORMED;
        }

        // ── Lock continuity: output must carry the same vault_lock code ──────
        // Compare code_hash (the vault lock binary hash) not the full lock hash,
        // because args legitimately change (is_frozen flips 0→1) so full lock
        // hashes differ between input and output. find_all_vaults_indexed already
        // filters by code_hash; this explicit check makes the invariant visible.
        let out_lock = match load_cell_lock(*out_idx, Source::Output) {
            Ok(l) => l,
            Err(_) => return ERROR_POOL_DATA_MALFORMED,
        };
        if out_lock.code_hash().as_slice() != old.vault_lock_code_hash {
            return ERROR_VAULT_DATA_MALFORMED;
        }

        // ── Type continuity: output must carry the same collateral type ───────
        let out_type_hash = match load_cell_type_hash(*out_idx, Source::Output) {
            Ok(Some(h)) => h,
            _ => return ERROR_POOL_DATA_MALFORMED,
        };
        if out_type_hash != old.asset_a_type_hash {
            return ERROR_VAULT_DATA_MALFORMED;
        }
    }

    // ── Physical Asset B balance must be strictly unchanged ─────────────────
    // Freeze moves no tokens — vault cells carry only collateral (Asset A), not
    // the loan token.  Without this check an attacker could drain Asset B from
    // the pool cell while passing the state-field accounting checks below.
    {
        let pool_lock = old.pool_lock_hash;
        let pool_asset_b_in = compute_sudt_total(Source::Input, &old.asset_b_type_hash, &pool_lock);
        let pool_asset_b_out =
            compute_sudt_total(Source::Output, &old.asset_b_type_hash, &pool_lock);
        if pool_asset_b_out != pool_asset_b_in {
            return ERROR_DEBT_ACCOUNTING;
        }
    }

    // ── Verify pool state changes ───────────────────────────────────────────
    // Debt and liquidity must be completely unchanged.
    if new.total_debt != old.total_debt {
        return ERROR_DEBT_ACCOUNTING;
    }
    if new.liquidity_available != old.liquidity_available {
        return ERROR_DEBT_ACCOUNTING;
    }
    // Rate contribution removed.
    if new.rate_net != old.rate_net - total_r_vault_removed {
        return ERROR_RATE_NET_MISMATCH;
    }
    // Interest updated to t_now minus the phantom overcharge.
    if new.interest_accrued != expected_interest {
        return ERROR_INTEREST_ACCOUNTING;
    }
    // Timestamp must advance to t_now.
    if new.timestamp_last_update != t_now {
        return ERROR_POOL_DATA_MALFORMED;
    }
    // Self-healing engine ticked forward.
    if new.k_min_current != healed.k_min_current
        || new.timestamp_last_haircut != healed.timestamp_last_haircut
    {
        return ERROR_POOL_DATA_MALFORMED;
    }

    // ── LP Supply Guard (Freeze must not mint or burn LP tokens) ─────────────
    check_lp_unchanged(old, new)
}

// ─────────────────────────────────────────────────────────────────────────────

/// Seconds that must pass after the last haircut before any healing begins.
/// 14 days = 14 * 24 * 60 * 60 = 1_209_600 seconds.
const HEALING_PERIOD: u64 = 1_209_600;

/// Absolute k-units subtracted from k_min_current per full healing period.
/// E.g., DECAY_STEP = 2 means each 14-day safe window lowers k_min by 2 units.
/// A 20-point spike fully heals in exactly 10 periods = 140 days.
const DECAY_STEP: u64 = 2;

/// Governance ceiling: k_min_current can never exceed k_target × K_MAX_SCALE.
/// Prevents a cascading black-swan from bricking the pool permanently (Death Spiral cap).
/// At 5×, if k_target = 50, max k_min_current = 250.
const K_MAX_SCALE: u64 = 5;

/// Downward Decay — Peace Time Healing.
///
/// After a market crash, the protocol slowly relaxes collateral requirements
/// back toward `k_min` (the governance floor) as time passes without loss.
///
/// **Equation:**
///   periods          = floor((t_now − timestamp_last_haircut) / HEALING_PERIOD)
///   healing_amount   = periods × DECAY_STEP
///   k_min_new        = max(k_min_base, k_min_current − healing_amount)
///
/// **O(1) complexity:** A single multiply + subtract. Zero loops, zero state bloat.
/// **Deterministic timeline:** A 20-point spike with DECAY_STEP=2 always heals in exactly
/// 10 periods (140 days), regardless of how often the pool is touched.
///
/// Advances `timestamp_last_haircut` by `periods × HEALING_PERIOD` only —
/// partial-period progress is banked, not lost.
fn apply_decay(old: &PoolData, t_now: u64) -> PoolData {
    let mut healed = old.clone();

    if t_now <= old.timestamp_last_haircut {
        return healed;
    }

    let elapsed = t_now - old.timestamp_last_haircut;
    let periods = elapsed / HEALING_PERIOD;

    if periods == 0 {
        return healed;
    }

    // O(1): single multiply + saturating sub — replaces the compound exponentiation loop.
    let healing_amount = periods.saturating_mul(DECAY_STEP);
    healed.k_min_current = old
        .k_min_current
        .saturating_sub(healing_amount)
        .max(old.k_min); // clamp at governance floor

    // Bank partial-period progress — advance clock by whole periods only.
    healed.timestamp_last_haircut = old.timestamp_last_haircut + periods * HEALING_PERIOD;

    healed
}

// ─────────────────────────────────────────────────────────────────────────────
// Type ID Creation Guard
// ─────────────────────────────────────────────────────────────────────────────

/// Validates that the pool cell is being created as a genuine Type ID singleton.
///
/// Enforces:
///   1. `args[0..32] == blake2b( inputs[0].outpoint(36 bytes) || output_index(u64 LE) )`
///      This ties the args cryptographically to a one-time-spendable input, making them globally unique.
///   2. Exactly one output in the transaction carries this pool's type hash.
///      Prevents duplicating the singleton in a single transaction.
fn validate_creation() -> i8 {
    // Load the pool script to read the args commitment.
    let script = match load_script() {
        Ok(s) => s,
        Err(_) => return ERROR_SYSCALL,
    };
    let args: Bytes = script.args().unpack();
    if args.len() < 32 {
        return ERROR_ENCODING;
    }
    let claimed_id = &args[0..32];

    // The commitment is derived from inputs[0] — the cell being consumed to fund creation.
    let first_input = match load_input(0, Source::Input) {
        Ok(i) => i,
        Err(_) => return ERROR_SYSCALL,
    };

    // Find which output index this pool cell occupies (by matching its own type hash).
    let own_type_hash = match load_script_hash() {
        Ok(h) => h,
        Err(_) => return ERROR_SYSCALL,
    };
    let mut output_index: u64 = u64::MAX;
    let mut singleton_count: usize = 0;
    let mut i: usize = 0;
    loop {
        match load_cell_type_hash(i, Source::Output) {
            Ok(Some(h)) if h == own_type_hash => {
                if output_index == u64::MAX {
                    output_index = i as u64; // first match = self
                }
                singleton_count += 1;
            }
            Ok(_) => {}
            Err(_) => break,
        }
        i += 1;
    }

    // Reject if this cell doesn't appear in outputs at all.
    if output_index == u64::MAX {
        return ERROR_SYSCALL;
    }
    // Enforce singleton: exactly one output may carry this type hash.
    if singleton_count != 1 {
        return ERROR_SINGLETON_VIOLATED;
    }

    // Recompute the expected Type ID commitment.
    // CKB spec: blake2b( outpoint_bytes[36] || output_index_u64_le[8] )
    let mut expected = [0u8; 32];
    let mut hasher = Blake2bBuilder::new(32)
        .personal(b"ckb-default-hash")
        .build();
    hasher.update(first_input.previous_output().as_slice()); // 36 bytes
    hasher.update(&output_index.to_le_bytes()); // 8 bytes
    hasher.finalize(&mut expected);

    if claimed_id != expected {
        return ERROR_INVALID_TYPE_ID;
    }

    0 // Creation valid — singleton established.
}

fn apply_adj(base: u128, adj: i128) -> u128 {
    if adj < 0 {
        base.saturating_sub(adj.unsigned_abs())
    } else {
        base.saturating_add(adj as u128)
    }
}

pub fn compute_sudt_total(source: Source, type_hash: &[u8; 32], lock_hash: &[u8; 32]) -> u128 {
    QueryIter::new(load_cell_type_hash, source)
        .enumerate()
        .filter(|(_, t)| t.as_ref() == Some(type_hash))
        .filter(|&(i, _)| load_cell_lock_hash(i, source).ok().as_ref() == Some(lock_hash))
        .filter_map(|(i, _)| {
            let data = load_cell_data(i, source);
            if data.is_err() {
                return None;
            }
            lenderr_common::utils::read_sudt_amount(&data.unwrap())
        })
        .sum()
}

/// Sum all LP xUDT cells across the **entire** transaction side (no lock filter).
///
/// LP tokens can sit in any user wallet, not just the pool cell, so we cannot
/// restrict by lock hash.  Summing every cell whose type hash matches the pool's
/// `lp_token_type_hash` gives us the correct side-by-side totals needed to
/// verify the net LP supply change enforced by pool_script.
///
/// The on-chain LP xUDT amount is stored at bytes [16..32] of the cell data
/// (xUDT layout: 16 bytes owner-mode args prefix + 16 bytes LE u128 amount).
/// We reuse `read_sudt_amount` which reads the first 16 bytes — valid for sUDT
/// cells whose data is just a u128.  LP tokens follow the same wire format.
fn sum_lp_xudt(type_hash: &[u8; 32], source: Source) -> u128 {
    QueryIter::new(load_cell_type_hash, source)
        .enumerate()
        .filter(|(_, t)| t.as_ref() == Some(type_hash))
        .filter_map(|(i, _)| {
            load_cell_data(i, source)
                .ok()
                .and_then(|d| lenderr_common::utils::read_sudt_amount(&d))
        })
        .sum()
}

/// Asserts that no LP tokens were minted or burned in this transaction.
///
/// Used by every action type that must leave LP supply completely untouched
/// (Borrow, Repay, Liquidate, Freeze).  Returns `ERROR_LP_MINT_INVALID` on
/// violation, `0` on success — callers propagate with `if r != 0 { return r; }`.
///
/// Two independent checks for defence in depth:
///  1. Physical scan  — Σ LP inputs == Σ LP outputs (no net creation/destruction)
///  2. State ledger   — pool's `total_lp_supply` field is unchanged
fn check_lp_unchanged(old: &PoolData, new: &PoolData) -> i8 {
    let lp_in = sum_lp_xudt(&old.lp_token_type_hash, Source::Input);
    let lp_out = sum_lp_xudt(&old.lp_token_type_hash, Source::Output);
    if lp_out != lp_in || new.total_lp_supply != old.total_lp_supply {
        return ERROR_LP_MINT_INVALID;
    }
    0
}

/// Extracts ALL vaults associated with this pool from the specified source,
/// returning each vault paired with its absolute cell index.
///
/// A vault cell must satisfy both:
///   • lock code_hash == `pool_data.vault_lock_code_hash`  (vault lock binary; constant across all borrowers)
///   • type hash      == `pool_data.asset_a_type_hash`     (collateral xUDT)
///
/// Loan metadata (principal, r_vault, t_created, t_exp, is_frozen) is read
/// from lock args [64..113]. The collateral amount is read from cell data
/// (standard xUDT 16-byte balance). The cell data field beyond those 16 bytes
/// is intentionally untouched, preserving compatibility with custom xUDT tokens
/// that store extension data after their balance.
///
/// Callers that don't need the index can strip it with `.map(|(_, vd)| vd)`.
fn find_all_vaults(pool_data: &PoolData, source: Source) -> Vec<(usize, VaultData)> {
    let mut result = Vec::new();
    let mut i = 0;
    loop {
        let lock = match load_cell_lock(i, source) {
            Ok(l) => l,
            Err(SysError::IndexOutOfBound) => break,
            Err(_) => {
                i += 1;
                continue;
            }
        };
        // Match by lock code_hash — constant across all vaults regardless of borrower args.
        if lock.code_hash().as_slice() != pool_data.vault_lock_code_hash {
            i += 1;
            continue;
        }
        // Verify the cell carries the collateral token type script.
        let type_matches = load_cell_type_hash(i, source)
            .ok()
            .and_then(|t| t)
            .map(|h| h == pool_data.asset_a_type_hash)
            .unwrap_or(false);
        if !type_matches {
            i += 1;
            continue;
        }
        // Read collateral amount from cell data (xUDT balance at bytes [0..16]).
        let collateral_amt = match load_cell_data(i, source) {
            Ok(data) => match read_sudt_amount(&data) {
                Some(amt) => amt,
                None => {
                    i += 1;
                    continue;
                }
            },
            Err(_) => {
                i += 1;
                continue;
            }
        };
        // Parse loan metadata from lock args [64..113].
        let lock_args: Bytes = lock.args().unpack();
        if let Some(vd) = VaultData::from_lock_args(lock_args.as_ref(), collateral_amt) {
            result.push((i, vd));
        }
        i += 1;
    }
    result
}
