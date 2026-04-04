use lenderr_common::pool_data::PoolData;

fn sample_pool() -> PoolData {
    PoolData {
        liquidity_available: 1_000_000,
        total_debt: 250_000,
        interest_accrued: 12_345,
        rate_net: 9_876_543_210,
        total_lp_supply: 777_777,
        timestamp_last_update: 1_710_000_000,
        k_min_current: 120,
        timestamp_last_haircut: 1_709_000_000,
        k_min: 100,
        k_target: 200,
        t_max: 30 * 86_400,
        base_rate: 200,
        max_rate: 2_000,
        asset_b_type_hash: [0x0b; 32],
        asset_a_type_hash: [0x0a; 32],
        lp_token_type_hash: [0x1f; 32],
        vault_lock_code_hash: [0x2c; 32],
        pool_lock_hash: [0x3d; 32],
    }
}

#[test]
fn test_pool_data_roundtrip_bytes() {
    let original = sample_pool();
    let encoded = original.to_bytes();
    let decoded = PoolData::from_bytes(&encoded).expect("valid molecule bytes");

    assert_eq!(decoded.liquidity_available, original.liquidity_available);
    assert_eq!(decoded.total_debt, original.total_debt);
    assert_eq!(decoded.interest_accrued, original.interest_accrued);
    assert_eq!(decoded.rate_net, original.rate_net);
    assert_eq!(decoded.total_lp_supply, original.total_lp_supply);
    assert_eq!(decoded.timestamp_last_update, original.timestamp_last_update);
    assert_eq!(decoded.k_min_current, original.k_min_current);
    assert_eq!(decoded.timestamp_last_haircut, original.timestamp_last_haircut);
    assert_eq!(decoded.k_min, original.k_min);
    assert_eq!(decoded.k_target, original.k_target);
    assert_eq!(decoded.t_max, original.t_max);
    assert_eq!(decoded.base_rate, original.base_rate);
    assert_eq!(decoded.max_rate, original.max_rate);
    assert_eq!(decoded.asset_b_type_hash, original.asset_b_type_hash);
    assert_eq!(decoded.asset_a_type_hash, original.asset_a_type_hash);
    assert_eq!(decoded.lp_token_type_hash, original.lp_token_type_hash);
    assert_eq!(decoded.vault_lock_code_hash, original.vault_lock_code_hash);
    assert_eq!(decoded.pool_lock_hash, original.pool_lock_hash);
}

#[test]
fn test_pool_data_from_bytes_invalid_payload() {
    let invalid = [0u8; 8];
    assert!(PoolData::from_bytes(&invalid).is_none());
}

#[test]
fn test_params_unchanged_ignores_dynamic_fields() {
    let original = sample_pool();
    let mut changed = sample_pool();
    changed.liquidity_available += 1;
    changed.total_debt += 1;
    changed.interest_accrued += 1;
    changed.rate_net += 1;
    changed.total_lp_supply += 1;
    changed.timestamp_last_update += 1;
    changed.k_min_current += 1;
    changed.timestamp_last_haircut += 1;

    assert!(original.params_unchanged(&changed));
}

#[test]
fn test_params_unchanged_detects_governance_change() {
    let original = sample_pool();
    let mut changed = sample_pool();
    changed.k_min += 1;

    assert!(!original.params_unchanged(&changed));
}

#[test]
fn test_params_unchanged_detects_hash_change() {
    let original = sample_pool();
    let mut changed = sample_pool();
    changed.pool_lock_hash[0] ^= 0xff;

    assert!(!original.params_unchanged(&changed));
}
