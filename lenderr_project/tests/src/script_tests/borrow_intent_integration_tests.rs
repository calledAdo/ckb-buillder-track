// Integration tests for `borrow_intent_lock`.
// The file is intentionally heavily commented so each setup step is explicit.

use ckb_testtool::{
    ckb_types::{
        bytes::Bytes,
        core::TransactionBuilder,
        packed::{CellInput, CellOutput},
        prelude::*,
    },
    context::Context,
};

const MAX_CYCLES: u64 = 15_000_000;
const ERROR_ENCODING: i8 = -1;
const ERROR_LOAN_NOT_DELIVERED: i8 = 31;
const ERROR_VAULT_NOT_CREATED: i8 = 32;

// Deploy compiled `borrow_intent_lock` RISC-V contract into the mock chain.
fn deploy_borrow_intent_lock(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    let bin = std::fs::read("../target/riscv64imac-unknown-none-elf/release/borrow_intent_lock")
        .expect("missing borrow_intent_lock binary; build intent_scripts for riscv target");
    context.deploy_cell(bin.into())
}

// Deploy always-success helper binary for mock owner/type/vault scripts.
fn deploy_always_success(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    context.deploy_cell(Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec()))
}

// Encode `borrow_intent_lock` args:
// [0..32]=owner_lock_hash
// [32..64]=asset_b_type_hash
// [64..96]=vault_lock_code_hash
// [96..112]=min_loan_b (le u128)
// [112..120]=min_duration (le u64)
// [120..136]=max_r_vault (le u128)
fn build_borrow_intent_args(
    owner_lock_hash: [u8; 32],
    asset_b_type_hash: [u8; 32],
    vault_lock_code_hash: [u8; 32],
    min_loan_b: u128,
    min_duration: u64,
    max_r_vault: u128,
) -> Bytes {
    let mut args = vec![0u8; 136];
    args[0..32].copy_from_slice(&owner_lock_hash);
    args[32..64].copy_from_slice(&asset_b_type_hash);
    args[64..96].copy_from_slice(&vault_lock_code_hash);
    args[96..112].copy_from_slice(&min_loan_b.to_le_bytes());
    args[112..120].copy_from_slice(&min_duration.to_le_bytes());
    args[120..136].copy_from_slice(&max_r_vault.to_le_bytes());
    Bytes::from(args)
}

// Encode vault-lock args used in borrow-intent validation.
fn build_vault_lock_args(
    borrower_lock_hash: [u8; 32],
    pool_type_hash: [u8; 32],
    principal: u128,
    r_vault: u128,
    t_created: u64,
    t_exp: u64,
    is_frozen: u8,
) -> Bytes {
    let mut args = vec![0u8; 113];
    args[0..32].copy_from_slice(&borrower_lock_hash);
    args[32..64].copy_from_slice(&pool_type_hash);
    args[64..80].copy_from_slice(&principal.to_le_bytes());
    args[80..96].copy_from_slice(&r_vault.to_le_bytes());
    args[96..104].copy_from_slice(&t_created.to_le_bytes());
    args[104..112].copy_from_slice(&t_exp.to_le_bytes());
    args[112] = is_frozen;
    Bytes::from(args)
}

#[test]
fn test_borrow_intent_cancel_path_by_owner_lockhash_presence() {
    // New isolated chain context.
    let mut context = Context::default();
    // Deploy tested lock + helper lock.
    let borrow_intent_out_point = deploy_borrow_intent_lock(&mut context);
    let always_success_out_point = deploy_always_success(&mut context);

    // Owner lock and its hash.
    let owner_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"owner"))
        .expect("build owner lock");
    let owner_lock_hash: [u8; 32] = owner_lock.calc_script_hash().unpack();

    // Loan token (asset_b) type and hash.
    let asset_b_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"asset-b"))
        .expect("build asset_b type");
    let asset_b_type_hash: [u8; 32] = asset_b_type_script.calc_script_hash().unpack();

    // Mock vault lock code hash used by borrow-intent args.
    let vault_lock_script = context
        .build_script(&always_success_out_point, Bytes::from(vec![0u8; 113]))
        .expect("build vault lock script");
    let vault_lock_code_hash: [u8; 32] = vault_lock_script.code_hash().unpack();

    // Build intent lock with normal constraints.
    let intent_lock = context
        .build_script(
            &borrow_intent_out_point,
            build_borrow_intent_args(
                owner_lock_hash,
                asset_b_type_hash,
                vault_lock_code_hash,
                1_000,
                100,
                10_000,
            ),
        )
        .expect("build borrow intent lock");

    // Input 0: borrow-intent cell being unlocked.
    let intent_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(intent_lock)
            .build(),
        Bytes::from(5_000u128.to_le_bytes().to_vec()),
    );
    // Input 1: owner-auth cell. Presence of this lock hash allows cancel path.
    let owner_auth_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(owner_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Minimal tx spending both inputs.
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(intent_input)
                .build(),
        )
        .input(
            CellInput::new_builder()
                .previous_output(owner_auth_input)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(owner_lock)
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();

    // Cancel path should pass.
    let tx = context.complete_tx(tx);
    context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("owner lockhash presence should allow cancel");
}

#[test]
fn test_borrow_intent_rejects_multiple_group_inputs() {
    // Fresh context and deployments.
    let mut context = Context::default();
    let borrow_intent_out_point = deploy_borrow_intent_lock(&mut context);
    let always_success_out_point = deploy_always_success(&mut context);

    // Standard owner/type/vault hashes.
    let owner_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"owner"))
        .expect("build owner lock");
    let owner_lock_hash: [u8; 32] = owner_lock.calc_script_hash().unpack();
    let asset_b_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"asset-b"))
        .expect("build asset_b type");
    let asset_b_type_hash: [u8; 32] = asset_b_type_script.calc_script_hash().unpack();
    let vault_lock_script = context
        .build_script(&always_success_out_point, Bytes::from(vec![0u8; 113]))
        .expect("build vault lock script");
    let vault_lock_code_hash: [u8; 32] = vault_lock_script.code_hash().unpack();

    // Build borrow intent lock.
    let intent_lock = context
        .build_script(
            &borrow_intent_out_point,
            build_borrow_intent_args(
                owner_lock_hash,
                asset_b_type_hash,
                vault_lock_code_hash,
                1_000,
                100,
                10_000,
            ),
        )
        .expect("build borrow intent lock");

    // Create two inputs in the same lock group (violates singleton rule).
    let intent_input_1 = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(intent_lock.clone())
            .build(),
        Bytes::from(5_000u128.to_le_bytes().to_vec()),
    );
    let intent_input_2 = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(intent_lock.clone())
            .build(),
        Bytes::from(5_000u128.to_le_bytes().to_vec()),
    );

    // Spend both in one tx.
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(intent_input_1)
                .build(),
        )
        .input(
            CellInput::new_builder()
                .previous_output(intent_input_2)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(intent_lock)
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();

    // Should fail with encoding error due to `single_group_input()` guard.
    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("error code {}", ERROR_ENCODING))
    );
}

#[test]
fn test_borrow_intent_aggregator_path_success() {
    // Fresh context and deployments.
    let mut context = Context::default();
    let borrow_intent_out_point = deploy_borrow_intent_lock(&mut context);
    let always_success_out_point = deploy_always_success(&mut context);

    // Standard owner lock/hash.
    let owner_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"owner"))
        .expect("build owner lock");
    let owner_lock_hash: [u8; 32] = owner_lock.calc_script_hash().unpack();

    // Loan token (asset_b) type/hash.
    let asset_b_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"asset-b"))
        .expect("build asset_b type");
    let asset_b_type_hash: [u8; 32] = asset_b_type_script.calc_script_hash().unpack();

    // Vault lock code hash to bind borrow-intent to expected vault script.
    let vault_lock_script = context
        .build_script(&always_success_out_point, Bytes::from(vec![0u8; 113]))
        .expect("build vault lock script");
    let vault_lock_code_hash: [u8; 32] = vault_lock_script.code_hash().unpack();

    // Constraint values used in both args and expected outputs.
    let min_loan = 1_000u128;
    let min_duration = 100u64;
    let max_r_vault = 10_000u128;
    let collateral = 5_000u128;

    // Build borrow-intent lock.
    let intent_lock = context
        .build_script(
            &borrow_intent_out_point,
            build_borrow_intent_args(
                owner_lock_hash,
                asset_b_type_hash,
                vault_lock_code_hash,
                min_loan,
                min_duration,
                max_r_vault,
            ),
        )
        .expect("build borrow intent lock");

    // Borrow-intent input carries collateral amount in cell data.
    let intent_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(intent_lock)
            .build(),
        Bytes::from(collateral.to_le_bytes().to_vec()),
    );

    // Output A: loan asset delivered to owner and above requested minimum.
    let loan_output = CellOutput::new_builder()
        .capacity(10_000u64)
        .lock(owner_lock.clone())
        .type_(Some(asset_b_type_script).pack())
        .build();
    let loan_data = Bytes::from((min_loan + 1).to_le_bytes().to_vec());

    // Output B: vault output satisfying borrower identity + duration + rate + collateral checks.
    let vault_output = CellOutput::new_builder()
        .capacity(10_000u64)
        .lock(
            context
                .build_script(
                    &always_success_out_point,
                    build_vault_lock_args(
                        owner_lock_hash,
                        [0x11; 32],
                        min_loan,
                        max_r_vault,
                        1_000,
                        1_000 + min_duration,
                        0,
                    ),
                )
                .expect("build vault output lock"),
        )
        .build();
    let vault_data = Bytes::from(collateral.to_le_bytes().to_vec());

    // Build and verify aggregator tx.
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(intent_input)
                .build(),
        )
        .output(loan_output)
        .output(vault_output)
        .output_data(loan_data.pack())
        .output_data(vault_data.pack())
        .build();

    // Aggregator path should pass.
    let tx = context.complete_tx(tx);
    context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("aggregator path should pass when loan and vault satisfy constraints");
}

#[test]
fn test_borrow_intent_rejects_when_loan_not_delivered() {
    // Fresh context and deployments.
    let mut context = Context::default();
    let borrow_intent_out_point = deploy_borrow_intent_lock(&mut context);
    let always_success_out_point = deploy_always_success(&mut context);

    // Standard owner/type/vault hashes.
    let owner_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"owner"))
        .expect("build owner lock");
    let owner_lock_hash: [u8; 32] = owner_lock.calc_script_hash().unpack();
    let asset_b_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"asset-b"))
        .expect("build asset_b type");
    let asset_b_type_hash: [u8; 32] = asset_b_type_script.calc_script_hash().unpack();
    let vault_lock_script = context
        .build_script(&always_success_out_point, Bytes::from(vec![0u8; 113]))
        .expect("build vault lock script");
    let vault_lock_code_hash: [u8; 32] = vault_lock_script.code_hash().unpack();

    // Build borrow intent lock.
    let intent_lock = context
        .build_script(
            &borrow_intent_out_point,
            build_borrow_intent_args(
                owner_lock_hash,
                asset_b_type_hash,
                vault_lock_code_hash,
                1_000,
                100,
                10_000,
            ),
        )
        .expect("build borrow intent lock");

    // Borrow-intent input.
    let intent_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(intent_lock)
            .build(),
        Bytes::from(5_000u128.to_le_bytes().to_vec()),
    );

    // Deliberately deliver loan output to wrong lock hash.
    let wrong_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"not-owner"))
        .expect("build wrong lock");
    let loan_output = CellOutput::new_builder()
        .capacity(10_000u64)
        .lock(wrong_lock)
        .type_(Some(asset_b_type_script).pack())
        .build();

    // No valid loan-delivery output exists.
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(intent_input)
                .build(),
        )
        .output(loan_output)
        .output_data(Bytes::from(2_000u128.to_le_bytes().to_vec()).pack())
        .build();

    // Must fail with loan-not-delivered error.
    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("error code {}", ERROR_LOAN_NOT_DELIVERED))
    );
}

#[test]
fn test_borrow_intent_rejects_when_vault_not_created() {
    // Fresh context and deployments.
    let mut context = Context::default();
    let borrow_intent_out_point = deploy_borrow_intent_lock(&mut context);
    let always_success_out_point = deploy_always_success(&mut context);

    // Standard owner/type/vault hashes.
    let owner_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"owner"))
        .expect("build owner lock");
    let owner_lock_hash: [u8; 32] = owner_lock.calc_script_hash().unpack();
    let asset_b_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"asset-b"))
        .expect("build asset_b type");
    let asset_b_type_hash: [u8; 32] = asset_b_type_script.calc_script_hash().unpack();
    let vault_lock_script = context
        .build_script(&always_success_out_point, Bytes::from(vec![0u8; 113]))
        .expect("build vault lock script");
    let vault_lock_code_hash: [u8; 32] = vault_lock_script.code_hash().unpack();

    let intent_lock = context
        .build_script(
            &borrow_intent_out_point,
            build_borrow_intent_args(
                owner_lock_hash,
                asset_b_type_hash,
                vault_lock_code_hash,
                1_000,
                100,
                10_000,
            ),
        )
        .expect("build borrow intent lock");

    // Borrow-intent input.
    let intent_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(intent_lock)
            .build(),
        Bytes::from(5_000u128.to_le_bytes().to_vec()),
    );

    // Loan delivery is valid.
    let loan_output = CellOutput::new_builder()
        .capacity(10_000u64)
        .lock(owner_lock)
        .type_(Some(asset_b_type_script).pack())
        .build();

    // No vault output is provided, so borrow intent must reject.
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(intent_input)
                .build(),
        )
        .output(loan_output)
        .output_data(Bytes::from(2_000u128.to_le_bytes().to_vec()).pack())
        .build();

    // Must fail with vault-not-created error.
    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("error code {}", ERROR_VAULT_NOT_CREATED))
    );
}
