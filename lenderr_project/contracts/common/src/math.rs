//! LendNerv Core Mathematics (`math.rs`)
//!
//! Technical Overview:
//! This module is the mathematical heart of the LendNerv priceless lending protocol.
//! Because the protocol operates without external price oracles, all risk management,
//! interest rates, and loan durations are completely deterministic and derived purely
//! from the token quantities submitted by the borrower in relation to the global pool state.
//!
//! Key components implemented in this module:
//! 1. **Utilization & Dynamic Interest Rates**: Calculates how heavily the pool is being borrowed from
//!    and scales the interest rate proportionally to economically disincentivize 100% utilization.
//! 2. **Priceless Origination (Time-Pricing)**: Eliminates traditional LTV (Loan-To-Value) ratios.
//!    Instead, it calculates loan *duration* dynamically based on the ratio of physical collateral tokens
//!    to physical loan tokens (Q), restricted by governance floors (k_min) and ceilings (k_target, t_max).
//! 3. **Exchange Rate Integrity**: Maintains the true value of LP tokens, ensuring that lenders are
//!    never diluted and always capture their exact slice of generated yield or socialized bad debt.
//! 4. **Lazy Evaluation Interest Accrual**: The centerpiece of the UTXO architecture. Because smart contracts
//!    are "asleep" until a user transaction occurs, interest is not incrementally calculated per block.
//!    Instead, the math retrospectively calculates the aggregate interest generated and elegantly "claws back"
//!    any theoretical interest attributed to vaults that silently expired while the pool was un-updated.

/// Fixed-point scale for rate calculations (10^12).
///
/// Floating-point numbers are not supported in determinisitic smart contracts.
/// All fractional math is handled by scaling up values, executing integer math,
/// and scaling down the result.
///
/// R_vault is stored as:
///   principal_tokens * rate_bps * RATE_PRECISION / (10_000 * SECONDS_PER_YEAR)
///
/// By using 10^12 precision, we prevent rounding-to-zero errors for very short timescales.
pub const RATE_PRECISION: u128 = 1_000_000_000_000; // 10^12

/// Scale for utilization.  0 = empty pool, UTIL_PRECISION = 100% utilized.
/// This prevents rounding errors when dealing with small pool utilizations.
pub const UTIL_PRECISION: u128 = 1_000_000; // 10^6

/// Constant defining the number of seconds in a standard 365-day calendar year.
/// Used to derive per-second linear interest rates from annualized basis points (bps).
pub const SECONDS_PER_YEAR: u128 = 31_536_000;

// ─────────────────────────────────────────────────────────────────────────────
// Origination math
// ─────────────────────────────────────────────────────────────────────────────

/// Calculates the current pool utilization, scaled by UTIL_PRECISION.
/// Formula: U = total_debt / (total_debt + liquidity_available)
pub fn utilization(total_debt: u128, liquidity_available: u128) -> u128 {
    // Total physical pool size (if all borrowers paid off their loans instantly).
    let total = total_debt + liquidity_available;
    // Handle the genesis state where the pool is entirely empty.
    if total == 0 {
        return 0; // Utilization is technically 0 if nothing exists.
    }
    // Multiply the numerator by the precision scale before division to retain accuracy.
    total_debt * UTIL_PRECISION / total
}

/// Calculates the specific annual interest rate (in basis points) for this exact millisecond.
/// Formula: R = base_rate + U * (max_rate − base_rate)
pub fn annual_rate_bps(u: u128, base_rate: u64, max_rate: u64) -> u128 {
    // Cast base parameters into wider bounds for precision integer math.
    let base = base_rate as u128;
    let max = max_rate as u128;
    // Add the scaled, utilization-dependent spread to the base rate.
    base + (u * (max - base)) / UTIL_PRECISION
}

/// Calculates `R_vault`: The hardcoded number of interest tokens this specific vault
/// will mathematically accrue every single second it remains active.
/// This value is frozen into the Vault cell's data upon origination.
pub fn compute_r_vault(principal: u128, rate_bps: u128) -> u128 {
    // Divide annualized bps by (10,000 * seconds_per_year) to get the per-second token rate,
    // explicitly multiplied by the global SCALE precision to prevent zero-rounding.
    principal * rate_bps * RATE_PRECISION / (10_000 * SECONDS_PER_YEAR)
}

/// Calculates the Priceless Loan Duration (D) in seconds.
/// This is the core protocol algorithm replacing oracles.
///   Formula: D = min(t_max,  t_max * ((Q − k_min) / (k_target − k_min)) * (1 − U))
/// Returns None if the request violates the minimum collateral threshold (Q < k_min), triggering a transaction revert.
pub fn compute_duration(
    collateral_a: u128,
    loan_b: u128,
    k_min: u64,
    k_target: u64,
    t_max: u64,
    u: u128, // utilization scaled by UTIL_PRECISION
) -> Option<u64> {
    // Guard clause: Cannot borrow zero tokens.
    if loan_b == 0 {
        return None;
    }

    // Step 1: Calculate the Quantity Ratio (Q).
    // E.g. borrower deposits 20,000 Asset A for 100 Asset B -> Q is 200.
    let q = collateral_a / loan_b; // integer quantity ratio

    // Cast strict governance parameters to wide precision.
    let k_min_u = k_min as u128;
    let k_target_u = k_target as u128;
    let t_max_u = t_max as u128;

    // Reject outright if the raw quantity ratio is worse than the governance-defined absolute bottom limit.
    if q < k_min_u {
        return None; // undercollateralized — reject
    }
    // Safety check against bad parameter governance changes.
    if k_target_u <= k_min_u {
        return None; // governance misconfiguration
    }

    // Step 2: Determine Collateral Factor
    // How far above the absolute minimum does this borrower sit?
    let collat_numer = (q - k_min_u) * UTIL_PRECISION;
    // What is the allowed sliding scale (the difference between safe target and minimum threshold)?
    let collat_denom = k_target_u - k_min_u;

    // Calculate fractional placement and clamp at 1.0 (UTIL_PRECISION).
    // Depositing way past the target does not give you an infinite loan duration; it is capped.
    let collat_factor = (collat_numer / collat_denom).min(UTIL_PRECISION);

    // Step 3: Determine Utilization Factor
    // This is the safety brake. As utilization approaches 1.0, this factor shrinks to zero.
    let util_factor = UTIL_PRECISION - u.min(UTIL_PRECISION);

    // Step 4: Final calculation using the scaled factors.
    // D = t_max * collat_factor * util_factor / UTIL_PRECISION^2
    let duration = t_max_u * collat_factor / UTIL_PRECISION * util_factor / UTIL_PRECISION;

    // Clamp the final resulting duration to never logically exceed T_Max under any anomaly.
    let duration = duration.min(t_max_u);

    // Provide the validated safe maximum time for the vault unlock script.
    Some(duration as u64)
}

// ─────────────────────────────────────────────────────────────────────────────
// LP token exchange rate
// ─────────────────────────────────────────────────────────────────────────────

/// Calculates the real-world value of a single LP token, scaled up heavily for precision.
///   Formula: rate = (liquidity + debt + interest) * RATE_PRECISION / lp_supply
pub fn exchange_rate(liquidity: u128, debt: u128, interest: u128, lp_supply: u128) -> u128 {
    // If this is the absolute first depositor ever, set a pristine baseline 1:1 ratio.
    if lp_supply == 0 {
        return RATE_PRECISION;
    }
    // The sum of the numerator represents the global "theoretical" pool enterprise value.
    (liquidity + debt + interest) * RATE_PRECISION / lp_supply
}

/// Computes how many LP tokens a lender must receive mathematically to preserve existing equity.
/// Used during the Deposit flow.
pub fn lp_tokens_for_deposit(deposit: u128, rate: u128) -> u128 {
    // Fail-safe / Initial condition handler if something zeroed out.
    if rate == 0 {
        return deposit;
    }
    // Cross-multiply by precision and divide by the current fractional exchange rate.
    deposit * RATE_PRECISION / rate
}

/// Computes how much physical Asset B a lender gets back when trading in their LP tokens.
/// Used during the Withdraw/Burn flow.
pub fn asset_b_for_burn(burn_amount: u128, rate: u128) -> u128 {
    // Standard yield realization algorithm, scaled back down.
    burn_amount * rate / RATE_PRECISION
}

// ─────────────────────────────────────────────────────────────────────────────
// Interest accrual (lazy evaluation)
// ─────────────────────────────────────────────────────────────────────────────

/// UTXO Lazy Accrual Core Algorithm
/// Calculates the exact Net Interest adjustment to apply to the global `interest_accrued` ledger.
/// Because Nervos transactions are asynchronous, we only run calculation leaps when someone touches the pool.
///
/// This specific function runs every time the pool is validated, whether creating or destroying loans.
/// - When NO vault is being closed (e.g. Deposit request): r_vault = 0, t_exp = 0.
/// - When A VAULT IS being closed (Repay/Liquidate): pass its specific terms.
///
/// The beauty of this formula is the "Clawback / Overcharge". If a vault silently defaulted exactly 4 weeks ago,
/// its `r_vault` component continued falsely inflating the pool's global expected value because no transaction fired.
/// This algorithm detects the stale vault being closed, realizes it stopped earning interest 4 weeks ago,
/// and subtracts that fake accumulated padding off the top.
pub fn interest_adjustment(
    rate_net: u128,
    t_last: u64,
    t_now: u64,
    r_vault: u128,
    t_exp: u64,
) -> i128 {
    // Safety check against chronometric drifts or consensus errors ensuring no negative time diffs.
    if t_now < t_last {
        return 0; // clock cannot go backwards
    }

    // Determine the total sleep duration since the last global state update.
    let time_window = (t_now - t_last) as u128;

    // Optimistic expectation: all currently tracked vaults earned interest normally during the sleep.
    let max_interest = rate_net * time_window / RATE_PRECISION;

    // Detect if the specific vault terminating in THIS transaction was dead during the sleep period.
    // We only care about overcharging if the vault reached its expiry (or t_last if it was already dead before).
    let overcharge_start = t_last.max(t_exp);

    // If the network is mathematically passed the expiry, we've actively been compounding false value.
    let overcharge = if t_now > overcharge_start {
        // Calculate exactly how much false, non-existent interest was accumulated.
        r_vault * (t_now - overcharge_start) as u128 / RATE_PRECISION
    } else {
        // Active early repayment, no overcharge.
        0
    };

    // Return the net shift (which can be a massive negative number if realizing a long-dead vault).
    max_interest as i128 - overcharge as i128
}

/// Extracts exactly what a single, specific borrower owes directly based on their vault terms.
/// This determines the exact amount of Asset B they must provide to successfully repay or liquidate.
pub fn vault_interest_owed(r_vault: u128, t_created: u64, t_settle: u64) -> u128 {
    // Cannot settle before creation timeframe.
    if t_settle <= t_created {
        return 0;
    }
    // Straight arithmetic applying their frozen interest rate against the active duration limits.
    // Early repayments use `t_now`. Defaults max out strictly at `t_exp`.
    let duration = (t_settle - t_created) as u128;
    r_vault * duration / RATE_PRECISION
}
