use ckb_testtool::{
    ckb_types::{
        bytes::Bytes,
        core::{TransactionBuilder, TransactionView},
        packed::*,
        prelude::*,
    },
    context::Context,
};

// Error codes based on the social_token script
const ERROR_ARGS_LENGTH: i8 = 5;
const ERROR_AMOUNT: i8 = 6;

// Max Cycles required for normal execution
const MAX_CYCLES: u64 = 10_000_000;

fn build_test_context(
    inputs: Vec<u128>,
    outputs: Vec<u128>,
    is_owner_mode: bool,
    custom_args: Option<Bytes>,
) -> (Context, TransactionView) {
    let mut context = Context::default();

    // Setup social_token script bin
    let token_bin = std::fs::read("../../target/riscv64imac-unknown-none-elf/release/social_token").expect("read social_token binaries");
    let token_out_point = context.deploy_cell(token_bin.into());

    // Setup an owner lock script (always deployed to simulate owner lock logic)
    let owner_lock_bin = Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec());
    let owner_lock_out_point = context.deploy_cell(owner_lock_bin);

    // Create the lock script for the owner
    let owner_lock_script = context
        .build_script(&owner_lock_out_point, Bytes::from("owner_args"))
        .expect("script");
    let owner_lock_hash = owner_lock_script.calc_script_hash();

    // Create a normal user lock script
    let user_lock_script = context
        .build_script(&owner_lock_out_point, Bytes::from("user_args"))
        .expect("script");

    // Arguments for the type script
    let type_args = custom_args.unwrap_or_else(|| owner_lock_hash.as_bytes());
    let token_type_script = context
        .build_script(&token_out_point, type_args)
        .expect("script");

    // Build inputs
    let inputs_cells: Vec<_> = inputs
        .into_iter()
        .enumerate()
        .map(|(i, amount)| {
            let lock = if is_owner_mode && i == 0 {
                owner_lock_script.clone()
            } else {
                user_lock_script.clone()
            };
            
            let data = Bytes::from(amount.to_le_bytes().to_vec());
            
            let cap: Uint64 = 1000u64.pack();
            let output = CellOutput::new_builder()
                .capacity(cap)
                .lock(lock)
                // Assuming it's a SUDT cell, we give it our token type script
                .type_(Some(token_type_script.clone()).pack())
                .build();
            (output, data)
        })
        .collect();

    let inputs: Vec<_> = inputs_cells
        .iter()
        .map(|(output, data)| context.create_cell(output.clone(), data.clone()))
        .collect();
    let inputs: Vec<CellInput> = inputs
        .iter()
        .map(|out_point| CellInput::new_builder().previous_output(out_point.clone()).build())
        .collect();

    // Build outputs
    let outputs_cells: Vec<_> = outputs
        .into_iter()
        .map(|amount| {
            let data = Bytes::from(amount.to_le_bytes().to_vec());
            let cap: Uint64 = 1000u64.pack();
            let output = CellOutput::new_builder()
                .capacity(cap)
                .lock(user_lock_script.clone())
                .type_(Some(token_type_script.clone()).pack())
                .build();
            (output, data)
        })
        .collect();

    let outputs: Vec<_> = outputs_cells.iter().map(|(output, _)| output.clone()).collect();
    let outputs_data: Vec<_> = outputs_cells.iter().map(|(_, data)| data.clone()).collect();

    // Build the transaction
    let tx = TransactionBuilder::default()
        .inputs(inputs)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .build();

    let tx = context.complete_tx(tx);

    (context, tx)
}

#[test]
fn test_owner_mode_mint() {
    // Mint 100 tokens as the owner (input=0, output=100)
    let (context, tx) = build_test_context(vec![0], vec![100], true, None);
    let cycles = context.verify_tx(&tx, MAX_CYCLES).expect("pass verification");
    println!("test_owner_mode_mint consumes cycles: {}", cycles);
}

#[test]
fn test_transfer_success() {
    // Normal transfer: input=100 -> output=100
    let (context, tx) = build_test_context(vec![100], vec![100], false, None);
    let cycles = context.verify_tx(&tx, MAX_CYCLES).expect("pass verification");
    println!("test_transfer_success consumes cycles: {}", cycles);
}

#[test]
fn test_transfer_burn() {
    // Normal transfer burning 50 tokens: input=100 -> output=50
    let (context, tx) = build_test_context(vec![100], vec![50], false, None);
    let cycles = context.verify_tx(&tx, MAX_CYCLES).expect("pass verification");
    println!("test_transfer_burn consumes cycles: {}", cycles);
}

#[test]
fn test_mint_fail_amount() {
    // Normal transfer attempting to mint 50 tokens: input=50 -> output=100
    let (context, tx) = build_test_context(vec![50], vec![100], false, None);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    let err_str = err.to_string();
    assert!(err_str.contains(&format!("error code {}", ERROR_AMOUNT)));
}

#[test]
fn test_fail_args_length() {
    // Invalid args length for the owner lock hash (short string)
    let (context, tx) = build_test_context(
        vec![50],
        vec![50],
        false,
        Some(Bytes::from("too_short_hash")),
    );
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    let err_str = err.to_string();
    assert!(err_str.contains(&format!("error code {}", ERROR_ARGS_LENGTH)));
}
