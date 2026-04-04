// Bring in the CKB testing harness and core packed/blockchain types.
use ckb_testtool::{
    // Core CKB types used to assemble scripts, cells, and transactions.
    ckb_types::{
        // `Bytes` is the raw byte container used for script args and cell data.
        bytes::Bytes,
        // Transaction builder used to construct a full tx in tests.
        core::TransactionBuilder,
        // Packed cell structures used by CKB.
        packed::{CellInput, CellOutput},
        // Prelude provides helper traits like `.pack()`.
        prelude::*,
    },
    // Execution context that deploys scripts and verifies txs.
    context::Context,
};

// Cycle budget for each verification run in this module.
const MAX_CYCLES: u64 = 15_000_000;
// Error code returned by `pool_lock` for structural invariants violations.
const ERROR_POOL_DATA_MALFORMED: i8 = 10;
// Error code returned by `pool_lock` for malformed args encoding.
const ERROR_ENCODING: i8 = -1;

/// Deploy compiled `pool_lock` binary into the in-memory chain context.
fn deploy_pool_lock(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    // Read the RISC-V contract binary from the standard build output path.
    let bin = std::fs::read("../target/riscv64imac-unknown-none-elf/release/pool_lock")
        // Fail fast with an actionable message when binary is missing.
        .expect(
            "missing pool_lock binary; build with cargo build --release --target riscv64imac-unknown-none-elf",
        );
    // Deploy binary as a code cell and return its out-point.
    context.deploy_cell(bin.into())
}

/// Deploy built-in always-success script and return its out-point.
fn deploy_always_success(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    // Convert built-in bytes to CKB `Bytes` then deploy.
    context.deploy_cell(Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec()))
}

/// Build a matching `(pool_lock_script, pool_type_script)` pair for test fixtures.
fn setup_scripts(
    context: &mut Context,
) -> (
    ckb_testtool::ckb_types::packed::Script,
    ckb_testtool::ckb_types::packed::Script,
) {
    // Deploy the lock under test.
    let pool_lock_out_point = deploy_pool_lock(context);
    // Deploy always-success to stand in as a simple type script.
    let always_success_out_point = deploy_always_success(context);

    // Build a deterministic "pool type" script (code=always_success, args="pool-type").
    let pool_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"pool-type"))
        .expect("build pool type script");
    // Compute script hash that must be referenced by pool_lock args.
    let pool_type_hash = pool_type_script.calc_script_hash();

    // Build `pool_lock` script where args are exactly the pool type hash bytes.
    let pool_lock_script = context
        .build_script(
            &pool_lock_out_point,
            Bytes::from(pool_type_hash.as_slice().to_vec()),
        )
        .expect("build pool lock script");

    // Return both scripts for convenient reuse in tests.
    (pool_lock_script, pool_type_script)
}

/// Happy path: one pool cell in input, one in output, same lock and same type.
#[test]
fn test_pool_lock_allows_singleton_continuity() {
    // New isolated chain simulation context.
    let mut context = Context::default();
    // Prepare valid scripts where lock args reference the pool type hash.
    let (pool_lock_script, pool_type_script) = setup_scripts(&mut context);

    // Create input pool cell locked by pool_lock and typed as pool_type.
    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            // Capacity is arbitrary but non-zero.
            .capacity(10_000u64)
            // Input lock is the exact pool_lock script.
            .lock(pool_lock_script.clone())
            // Input type is the referenced pool type script.
            .type_(Some(pool_type_script.clone()).pack())
            .build(),
        // Empty data is enough because pool_lock checks structure, not accounting data.
        Bytes::new(),
    );

    // Consume that input cell in tx.
    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Recreate one output pool cell with unchanged lock and type.
    let output = CellOutput::new_builder()
        .capacity(10_000u64)
        .lock(pool_lock_script)
        .type_(Some(pool_type_script).pack())
        .build();

    // Assemble transaction with one input and one output.
    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(Bytes::new().pack())
        .build();

    // Fill required deps/witnesses defaults.
    let tx = context.complete_tx(tx);
    // Must succeed: singleton continuity is preserved.
    context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("pool lock should allow unchanged singleton");
}

/// Negative path: output pool cell lock changed to a different script.
#[test]
fn test_pool_lock_rejects_output_lock_hijack() {
    // New isolated context.
    let mut context = Context::default();
    // Valid pool lock + type scripts.
    let (pool_lock_script, pool_type_script) = setup_scripts(&mut context);
    // Deploy always-success again to synthesize a different output lock.
    let always_success_out_point = deploy_always_success(&mut context);

    // Build a lock script that is intentionally NOT pool_lock.
    let hijack_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"hijack"))
        .expect("build hijack lock");

    // Create valid input pool cell.
    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(pool_lock_script.clone())
            .type_(Some(pool_type_script.clone()).pack())
            .build(),
        Bytes::new(),
    );

    // Build tx that swaps output lock to hijack_lock.
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(input_out_point)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(hijack_lock)
                .type_(Some(pool_type_script).pack())
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();

    // Finalize and execute.
    let tx = context.complete_tx(tx);
    // This path must fail because lock continuity was broken.
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    // Assert script returned expected structural error code.
    assert!(
        err.to_string()
            .contains(&format!("error code {}", ERROR_POOL_DATA_MALFORMED))
    );
}

/// Negative path: pool_lock args length is not 32 bytes (invalid encoding).
#[test]
fn test_pool_lock_rejects_invalid_args_length() {
    // New context for isolation.
    let mut context = Context::default();
    // Deploy code cells used for scripts.
    let pool_lock_out_point = deploy_pool_lock(&mut context);
    let always_success_out_point = deploy_always_success(&mut context);

    // Build pool type script used by the pool cell.
    let pool_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"pool-type"))
        .expect("build pool type script");

    // Build malformed pool lock with short args (must be exactly 32 bytes hash).
    let malformed_pool_lock = context
        .build_script(&pool_lock_out_point, Bytes::from_static(b"too-short"))
        .expect("build malformed pool lock");

    // Create input pool cell with malformed lock args.
    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(malformed_pool_lock.clone())
            .type_(Some(pool_type_script.clone()).pack())
            .build(),
        Bytes::new(),
    );

    // Build tx that preserves that malformed lock across input/output.
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(input_out_point)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(malformed_pool_lock)
                .type_(Some(pool_type_script).pack())
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();

    // Execute and assert encoding error.
    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("error code {}", ERROR_ENCODING))
    );
}

/// Negative path: args-referenced type hash appears in more than one output cell.
#[test]
fn test_pool_lock_rejects_multiple_cells_with_reference_type_hash_in_outputs() {
    // New isolated context.
    let mut context = Context::default();
    // Valid script pair.
    let (pool_lock_script, pool_type_script) = setup_scripts(&mut context);

    // Create one valid input pool cell.
    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(pool_lock_script.clone())
            .type_(Some(pool_type_script.clone()).pack())
            .build(),
        Bytes::new(),
    );

    // Build tx with TWO outputs carrying the same referenced pool type hash.
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(input_out_point)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(pool_lock_script.clone())
                .type_(Some(pool_type_script.clone()).pack())
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(pool_lock_script)
                .type_(Some(pool_type_script).pack())
                .build(),
        )
        // Data placeholders for each output cell.
        .output_data(Bytes::new().pack())
        .output_data(Bytes::new().pack())
        .build();

    // Must fail because output singleton count must be exactly 1.
    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("error code {}", ERROR_POOL_DATA_MALFORMED))
    );
}

/// Negative path: args-referenced type hash appears in more than one input cell.
#[test]
fn test_pool_lock_rejects_multiple_cells_with_reference_type_hash_in_inputs() {
    // New isolated context.
    let mut context = Context::default();
    // Valid script pair.
    let (pool_lock_script, pool_type_script) = setup_scripts(&mut context);

    // Create first input pool cell with referenced type hash.
    let input_out_point_1 = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(pool_lock_script.clone())
            .type_(Some(pool_type_script.clone()).pack())
            .build(),
        Bytes::new(),
    );
    // Create second input pool cell with same referenced type hash.
    let input_out_point_2 = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(pool_lock_script.clone())
            .type_(Some(pool_type_script.clone()).pack())
            .build(),
        Bytes::new(),
    );

    // Build tx with TWO matching inputs and one matching output.
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(input_out_point_1)
                .build(),
        )
        .input(
            CellInput::new_builder()
                .previous_output(input_out_point_2)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(pool_lock_script)
                .type_(Some(pool_type_script).pack())
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();

    // Must fail because input singleton count must be exactly 1.
    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("error code {}", ERROR_POOL_DATA_MALFORMED))
    );
}
