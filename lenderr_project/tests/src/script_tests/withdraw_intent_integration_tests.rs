// Integration tests for `withdraw_intent_lock`.
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

// Standard cycle budget used across integration tests.
const MAX_CYCLES: u64 = 15_000_000;

// Expected script errors from `lenderr_common::errors`.
const ERROR_ENCODING: i8 = -1;
const ERROR_ASSET_NOT_DELIVERED: i8 = 34;

// Deploy compiled `withdraw_intent_lock` RISC-V contract into the mock chain.
fn deploy_withdraw_intent_lock(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    let bin =
        std::fs::read("../target/riscv64imac-unknown-none-elf/release/withdraw_intent_lock")
            .expect("missing withdraw_intent_lock binary; build intent_scripts for riscv target");
    context.deploy_cell(bin.into())
}

// Deploy always-success helper binary for mock owner/type scripts.
fn deploy_always_success(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    context.deploy_cell(Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec()))
}

// Encode `withdraw_intent_lock` args:
// [0..32]=owner_lock_hash, [32..64]=asset_b_type_hash, [64..80]=min_asset_b_amount(le u128)
fn build_withdraw_intent_args(
    owner_lock_hash: [u8; 32],
    asset_b_type_hash: [u8; 32],
    min_asset_b_amount: u128,
) -> Bytes {
    let mut args = vec![0u8; 80];
    args[0..32].copy_from_slice(&owner_lock_hash);
    args[32..64].copy_from_slice(&asset_b_type_hash);
    args[64..80].copy_from_slice(&min_asset_b_amount.to_le_bytes());
    Bytes::from(args)
}

#[test]
fn test_withdraw_intent_cancel_path_by_owner_lockhash_presence() {
    // New isolated chain context.
    let mut context = Context::default();

    // Deploy tested lock and helper scripts.
    let withdraw_intent_out_point = deploy_withdraw_intent_lock(&mut context);
    let always_success_out_point = deploy_always_success(&mut context);

    // Create an owner lock and derive its lock hash for args.
    let owner_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"owner"))
        .expect("build owner lock");
    let owner_lock_hash: [u8; 32] = owner_lock.calc_script_hash().unpack();

    // Create an asset-b type script and hash used in args.
    let asset_b_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"asset-b"))
        .expect("build asset-b type");
    let asset_b_type_hash: [u8; 32] = asset_b_type_script.calc_script_hash().unpack();

    // Build the withdraw-intent lock from encoded args.
    let intent_lock = context
        .build_script(
            &withdraw_intent_out_point,
            build_withdraw_intent_args(owner_lock_hash, asset_b_type_hash, 1_000),
        )
        .expect("build withdraw intent lock");

    // Input 0: withdraw-intent cell being unlocked.
    let intent_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(intent_lock)
            .build(),
        // Data is xUDT amount for LP being withdrawn, value not used by this lock.
        Bytes::from(5_000u128.to_le_bytes().to_vec()),
    );

    // Input 1: owner-auth input proving `owner_lock_hash` presence in inputs.
    let owner_auth_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(owner_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Build minimal transaction that spends both inputs.
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

    // Complete deps and verify: cancel path should succeed.
    let tx = context.complete_tx(tx);
    context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("owner lockhash presence should allow cancel");
}

#[test]
fn test_withdraw_intent_aggregator_path_success() {
    // Fresh context.
    let mut context = Context::default();
    let withdraw_intent_out_point = deploy_withdraw_intent_lock(&mut context);
    let always_success_out_point = deploy_always_success(&mut context);

    // Owner lock and hash.
    let owner_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"owner"))
        .expect("build owner lock");
    let owner_lock_hash: [u8; 32] = owner_lock.calc_script_hash().unpack();

    // Underlying asset type script and hash.
    let asset_b_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"asset-b"))
        .expect("build asset-b type");
    let asset_b_type_hash: [u8; 32] = asset_b_type_script.calc_script_hash().unpack();

    // Lock args ask for at least 1_000 underlying asset.
    let min_asset_amount = 1_000u128;
    let intent_lock = context
        .build_script(
            &withdraw_intent_out_point,
            build_withdraw_intent_args(owner_lock_hash, asset_b_type_hash, min_asset_amount),
        )
        .expect("build withdraw intent lock");

    // Withdraw-intent input cell.
    let intent_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(intent_lock)
            .build(),
        Bytes::from(5_000u128.to_le_bytes().to_vec()),
    );

    // Aggregator output delivers underlying asset to owner with enough amount.
    let asset_output = CellOutput::new_builder()
        .capacity(10_000u64)
        .lock(owner_lock)
        .type_(Some(asset_b_type_script).pack())
        .build();
    let asset_output_data = Bytes::from((min_asset_amount + 10).to_le_bytes().to_vec());

    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(intent_input)
                .build(),
        )
        .output(asset_output)
        .output_data(asset_output_data.pack())
        .build();

    let tx = context.complete_tx(tx);
    context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("aggregator path should pass when asset output satisfies floor");
}

#[test]
fn test_withdraw_intent_rejects_when_asset_not_delivered_to_owner() {
    // Fresh context.
    let mut context = Context::default();
    let withdraw_intent_out_point = deploy_withdraw_intent_lock(&mut context);
    let always_success_out_point = deploy_always_success(&mut context);

    // Owner setup.
    let owner_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"owner"))
        .expect("build owner lock");
    let owner_lock_hash: [u8; 32] = owner_lock.calc_script_hash().unpack();

    // Asset type setup.
    let asset_b_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"asset-b"))
        .expect("build asset-b type");
    let asset_b_type_hash: [u8; 32] = asset_b_type_script.calc_script_hash().unpack();

    // Intent lock creation.
    let intent_lock = context
        .build_script(
            &withdraw_intent_out_point,
            build_withdraw_intent_args(owner_lock_hash, asset_b_type_hash, 1_000),
        )
        .expect("build withdraw intent lock");

    // Intent input cell.
    let intent_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(intent_lock)
            .build(),
        Bytes::from(5_000u128.to_le_bytes().to_vec()),
    );

    // Asset output uses wrong recipient lock hash.
    let wrong_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"not-owner"))
        .expect("build wrong lock");
    let wrong_asset_output = CellOutput::new_builder()
        .capacity(10_000u64)
        .lock(wrong_lock)
        .type_(Some(asset_b_type_script).pack())
        .build();

    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(intent_input)
                .build(),
        )
        .output(wrong_asset_output)
        .output_data(Bytes::from(2_000u128.to_le_bytes().to_vec()).pack())
        .build();

    // Must fail with asset-not-delivered code.
    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("error code {}", ERROR_ASSET_NOT_DELIVERED))
    );
}

#[test]
fn test_withdraw_intent_rejects_invalid_args_length() {
    // Fresh context.
    let mut context = Context::default();
    let withdraw_intent_out_point = deploy_withdraw_intent_lock(&mut context);

    // Build lock with malformed short args.
    let bad_lock = context
        .build_script(&withdraw_intent_out_point, Bytes::from(vec![0u8; 1]))
        .expect("build malformed lock");

    // Build one input under malformed lock.
    let bad_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(bad_lock)
            .build(),
        Bytes::new(),
    );

    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(bad_input)
                .build(),
        )
        .build();

    // Expect encoding error.
    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("error code {}", ERROR_ENCODING))
    );
}
