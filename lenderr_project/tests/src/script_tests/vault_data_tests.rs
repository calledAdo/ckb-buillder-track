use lenderr_common::vault_data::{VaultData, VAULT_ARGS_LEN};

fn build_lock_args(
    borrower_lock_hash: [u8; 32],
    pool_type_hash: [u8; 32],
    principal: u128,
    r_vault: u128,
    t_created: u64,
    t_exp: u64,
    is_frozen: u8,
) -> Vec<u8> {
    let mut args = vec![0u8; VAULT_ARGS_LEN];

    args[0..32].copy_from_slice(&borrower_lock_hash);
    args[32..64].copy_from_slice(&pool_type_hash);
    args[64..80].copy_from_slice(&principal.to_le_bytes());
    args[80..96].copy_from_slice(&r_vault.to_le_bytes());
    args[96..104].copy_from_slice(&t_created.to_le_bytes());
    args[104..112].copy_from_slice(&t_exp.to_le_bytes());
    args[112] = is_frozen;

    args
}

#[test]
fn test_vault_from_lock_args_parses_fields() {
    let borrower_lock_hash = [0xaa; 32];
    let pool_type_hash = [0xbb; 32];
    let principal = 120_000u128;
    let r_vault = 123_456_789u128;
    let t_created = 1_700_000_000u64;
    let t_exp = 1_700_086_400u64;
    let is_frozen = 1u8;
    let collateral_amt = 42_000u128;

    let args = build_lock_args(
        borrower_lock_hash,
        pool_type_hash,
        principal,
        r_vault,
        t_created,
        t_exp,
        is_frozen,
    );
    let parsed = VaultData::from_lock_args(&args, collateral_amt).expect("valid args");

    assert_eq!(parsed.borrower_lock_hash, borrower_lock_hash);
    assert_eq!(parsed.pool_type_hash, pool_type_hash);
    assert_eq!(parsed.collateral_amt, collateral_amt);
    assert_eq!(parsed.principal, principal);
    assert_eq!(parsed.r_vault, r_vault);
    assert_eq!(parsed.t_created, t_created);
    assert_eq!(parsed.t_exp, t_exp);
    assert_eq!(parsed.is_frozen, is_frozen);
}

#[test]
fn test_vault_from_lock_args_rejects_short_args() {
    let short = vec![0u8; VAULT_ARGS_LEN - 1];
    assert!(VaultData::from_lock_args(&short, 100).is_none());
}

#[test]
fn test_vault_to_args_extension_encoding() {
    let vault = VaultData {
        borrower_lock_hash: [0xaa; 32],
        pool_type_hash: [0xbb; 32],
        collateral_amt: 777,
        principal: 10_000,
        r_vault: 55_555,
        t_created: 1_710_000_000,
        t_exp: 1_710_010_000,
        is_frozen: 0,
    };

    let ext = vault.to_args_extension();
    assert_eq!(ext.len(), VAULT_ARGS_LEN - 64);

    let mut full_args = vec![0u8; VAULT_ARGS_LEN];
    full_args[0..32].copy_from_slice(&vault.borrower_lock_hash);
    full_args[32..64].copy_from_slice(&vault.pool_type_hash);
    full_args[64..113].copy_from_slice(&ext);

    let reparsed = VaultData::from_lock_args(&full_args, vault.collateral_amt).expect("reparse");
    assert_eq!(reparsed.borrower_lock_hash, vault.borrower_lock_hash);
    assert_eq!(reparsed.pool_type_hash, vault.pool_type_hash);
    assert_eq!(reparsed.principal, vault.principal);
    assert_eq!(reparsed.r_vault, vault.r_vault);
    assert_eq!(reparsed.t_created, vault.t_created);
    assert_eq!(reparsed.t_exp, vault.t_exp);
    assert_eq!(reparsed.is_frozen, vault.is_frozen);
}
