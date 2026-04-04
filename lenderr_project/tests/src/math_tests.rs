use lenderr_common::math::*;

// ── utilization ───────────────────────────────────────────────────────────────

#[test]
fn test_utilization_empty_pool() {
    assert_eq!(utilization(0, 10_000), 0);
}

#[test]
fn test_utilization_full() {
    // All borrowed — 100% utilization
    assert_eq!(utilization(10_000, 0), UTIL_PRECISION);
}

#[test]
fn test_utilization_20_percent() {
    let u = utilization(2_000, 8_000);
    assert_eq!(u, UTIL_PRECISION / 5); // 200_000
}

// ── annual_rate_bps ───────────────────────────────────────────────────────────

#[test]
fn test_rate_at_zero_utilization() {
    let r = annual_rate_bps(0, 200, 2000);
    assert_eq!(r, 200); // base rate
}

#[test]
fn test_rate_at_full_utilization() {
    let r = annual_rate_bps(UTIL_PRECISION, 200, 2000);
    assert_eq!(r, 2000); // max rate
}

#[test]
fn test_rate_at_half_utilization() {
    let r = annual_rate_bps(UTIL_PRECISION / 2, 200, 2000);
    assert_eq!(r, 1100); // midpoint: 200 + 900 = 1100 bps
}

// ── compute_duration ──────────────────────────────────────────────────────────

#[test]
fn test_duration_undercollateralized_returns_none() {
    // Q = 50 / 1 = 50, k_min = 100 → reject
    let d = compute_duration(50, 1, 100, 200, 30 * 86400, 0);
    assert!(d.is_none());
}

#[test]
fn test_duration_at_k_min_returns_zero() {
    // Q == k_min → numerator (Q - k_min) == 0 → D = 0
    let d = compute_duration(100, 1, 100, 200, 30 * 86400, 0);
    assert_eq!(d, Some(0));
}

#[test]
fn test_duration_at_k_target_low_utilization() {
    // Q == k_target, U ≈ 0 → D approaches t_max
    let t_max = 30 * 86_400u64; // 30 days
    let d = compute_duration(200, 1, 100, 200, t_max, 0);
    assert_eq!(d, Some(t_max));
}

#[test]
fn test_duration_shrinks_at_high_utilization() {
    let t_max = 30 * 86_400u64;
    // Same collateral, but U = 80%
    let u_80 = UTIL_PRECISION * 4 / 5;
    let d = compute_duration(200, 1, 100, 200, t_max, u_80);
    let expected = t_max / 5; // (1 - 0.8) = 0.2 of t_max
    assert_eq!(d, Some(expected));
}

// ── exchange_rate ─────────────────────────────────────────────────────────────

#[test]
fn test_exchange_rate_empty_pool_is_one() {
    let r = exchange_rate(0, 0, 0, 0);
    assert_eq!(r, RATE_PRECISION);
}

#[test]
fn test_exchange_rate_no_interest() {
    // 10_000 liquidity, 0 debt/interest, 10_000 LP supply → rate = 1.0
    let r = exchange_rate(10_000, 0, 0, 10_000);
    assert_eq!(r, RATE_PRECISION);
}

#[test]
fn test_exchange_rate_with_interest() {
    // 10_000 liquidity, 2_000 debt, 100 interest accrued, 10_000 LP
    // Rate = 12_100 / 10_000 = 1.21 scaled
    let r = exchange_rate(10_000, 2_000, 100, 10_000);
    assert_eq!(r, RATE_PRECISION * 12_100 / 10_000);
}

// ── lp_tokens_for_deposit ─────────────────────────────────────────────────────

#[test]
fn test_lp_tokens_at_1_to_1_rate() {
    let tokens = lp_tokens_for_deposit(1_000, RATE_PRECISION);
    assert_eq!(tokens, 1_000);
}

#[test]
fn test_lp_tokens_at_1_21_rate() {
    // Deposit 1_000, rate 1.21 → fewer LP tokens (latecomers buy in pricier)
    let rate = RATE_PRECISION * 121 / 100;
    let tokens = lp_tokens_for_deposit(1_000, rate);
    // 1_000 * RATE_PRECISION / (1.21 * RATE_PRECISION) = 826 (integer division)
    assert_eq!(tokens, 826);
}

// ── interest_adjustment ───────────────────────────────────────────────────────

#[test]
fn test_adjustment_no_vault_closure() {
    // rate_net accrues for 100 seconds, no expired vault
    let rate_net = RATE_PRECISION; // 1 token per second after scaling
    let adj = interest_adjustment(rate_net, 0, 100, 0, 0);
    assert_eq!(adj, 100); // 1 * 100 = 100 tokens
}

#[test]
fn test_adjustment_with_expired_vault_before_last_update() {
    // Vault expired at t=10, last update was t=20, now t=30.
    // rate_net = 2 (vault A rate=1 + vault B rate=1), r_vault_A = 1 (expired at 10).
    // max_interest = 2 * (30-20) = 20
    // overcharge = 1 * (30 - max(20,10)) = 1 * 10 = 10
    // true = 20 - 10 = 10  (only vault B's contribution)
    let rate_net = 2 * RATE_PRECISION;
    let r_vault  = RATE_PRECISION;
    let adj = interest_adjustment(rate_net, 20, 30, r_vault, 10);
    assert_eq!(adj, 10);
}

#[test]
fn test_adjustment_with_vault_expiring_in_window() {
    // Vault expires at t=25, last update t=20, now t=30.
    // rate_net = 2, r_vault = 1
    // max_interest = 2 * 10 = 20
    // overcharge = 1 * (30 - max(20,25)) = 1 * 5 = 5
    // true = 15  (vault B contributed 10, vault A contributed 5 before expiry)
    let rate_net = 2 * RATE_PRECISION;
    let r_vault  = RATE_PRECISION;
    let adj = interest_adjustment(rate_net, 20, 30, r_vault, 25);
    assert_eq!(adj, 15);
}

// ── vault_interest_owed ───────────────────────────────────────────────────────

#[test]
fn test_vault_interest_owed() {
    // r_vault = RATE_PRECISION (1 token/sec), active for 500 seconds
    let owed = vault_interest_owed(RATE_PRECISION, 0, 500);
    assert_eq!(owed, 500);
}
