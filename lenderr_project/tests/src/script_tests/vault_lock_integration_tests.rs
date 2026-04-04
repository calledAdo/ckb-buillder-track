use ckb_testtool::{
    ckb_types::{
        bytes::Bytes,
        core::{HeaderBuilder, TransactionBuilder},
        packed::{CellInput, CellOutput},
        prelude::*,
    },
    context::Context,
};

const MAX_CYCLES: u64 = 15_000_000;
const ERROR_WRONG_POOL_RECIPIENT: i8 = 24;
const ERROR_VAULT_NOT_EXPIRED: i8 = 21;

fn deploy_vault_lock(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    let bin = std::fs::read("../target/riscv64imac-unknown-none-elf/release/vault_lock")
        .expect("missing vault_lock binary; build with cargo build -p vault_scripts --release --target riscv64imac-unknown-none-elf");
    context.deploy_cell(bin.into())
}

fn deploy_always_success(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    context.deploy_cell(Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec()))
}

fn build_vault_args(borrower_lock_hash: [u8; 32], pool_type_hash: [u8; 32], t_exp: u64) -> Bytes {
    let mut args = vec![0u8; 113];
    args[0..32].copy_from_slice(&borrower_lock_hash);
    args[32..64].copy_from_slice(&pool_type_hash);
    args[104..112].copy_from_slice(&t_exp.to_le_bytes());
    Bytes::from(args)
}

fn setup_vault_lock_script(
    context: &mut Context,
    borrower_lock_hash: [u8; 32],
    pool_type_hash: [u8; 32],
    t_exp: u64,
) -> ckb_testtool::ckb_types::packed::Script {
    let vault_lock_out_point = deploy_vault_lock(context);
    let args = build_vault_args(borrower_lock_hash, pool_type_hash, t_exp);
    context
        .build_script(&vault_lock_out_point, args)
        .expect("build vault lock script")
}

fn add_header_dep(
    context: &mut Context,
    tx: ckb_testtool::ckb_types::core::TransactionView,
    t_now_secs: u64,
) -> ckb_testtool::ckb_types::core::TransactionView {
    let header = HeaderBuilder::default()
        .timestamp(t_now_secs * 1000)
        .build();
    let header_hash = header.hash();
    context.insert_header(header);

    tx.as_advanced_builder().header_dep(header_hash).build()
}

#[test]
fn test_vault_lock_allows_borrower_lockhash_presence_before_expiry() {
    let mut context = Context::default();
    let always_success_out_point = deploy_always_success(&mut context);

    let borrower_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"borrower"))
        .expect("build borrower lock");
    let borrower_lock_hash: [u8; 32] = borrower_lock.calc_script_hash().unpack();

    let pool_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"pool-type"))
        .expect("build pool type script");
    let pool_type_hash: [u8; 32] = pool_type_script.calc_script_hash().unpack();

    let t_exp = 2_000_000_000u64;
    let vault_lock_script =
        setup_vault_lock_script(&mut context, borrower_lock_hash, pool_type_hash, t_exp);

    let vault_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(vault_lock_script.clone())
            .build(),
        Bytes::new(),
    );
    // Pool presence is required unconditionally.
    let pool_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(vault_lock_script)
            .type_(Some(pool_type_script).pack())
            .build(),
        Bytes::new(),
    );
    // Borrower lock presence authorizes early path.
    let borrower_auth_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(borrower_lock.clone())
            .build(),
        Bytes::new(),
    );

    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(vault_input)
                .build(),
        )
        .input(CellInput::new_builder().previous_output(pool_input).build())
        .input(
            CellInput::new_builder()
                .previous_output(borrower_auth_input)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(borrower_lock)
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();

    let tx = add_header_dep(&mut context, tx, t_exp - 1);
    let tx = context.complete_tx(tx);
    context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("borrower lock-hash presence should authorize before expiry");
}

#[test]
fn test_vault_lock_rejects_without_borrower_lockhash_before_expiry() {
    let mut context = Context::default();
    let always_success_out_point = deploy_always_success(&mut context);

    let borrower_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"borrower"))
        .expect("build borrower lock");
    let borrower_lock_hash: [u8; 32] = borrower_lock.calc_script_hash().unpack();

    let pool_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"pool-type"))
        .expect("build pool type script");
    let pool_type_hash: [u8; 32] = pool_type_script.calc_script_hash().unpack();

    let t_exp = 2_000_000_000u64;
    let vault_lock_script =
        setup_vault_lock_script(&mut context, borrower_lock_hash, pool_type_hash, t_exp);

    let vault_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(vault_lock_script.clone())
            .build(),
        Bytes::new(),
    );
    let pool_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(vault_lock_script.clone())
            .type_(Some(pool_type_script).pack())
            .build(),
        Bytes::new(),
    );
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(vault_input)
                .build(),
        )
        .input(
            CellInput::new_builder()
                .previous_output(pool_input)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(borrower_lock)
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();

    let tx = add_header_dep(&mut context, tx, t_exp - 1);
    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(err
        .to_string()
        .contains(&format!("error code {}", ERROR_VAULT_NOT_EXPIRED)));
}

#[test]
fn test_vault_lock_rejects_without_pool_input() {
    let mut context = Context::default();
    let always_success_out_point = deploy_always_success(&mut context);

    let borrower_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"borrower"))
        .expect("build borrower lock");
    let borrower_lock_hash: [u8; 32] = borrower_lock.calc_script_hash().unpack();

    let pool_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"pool-type"))
        .expect("build pool type script");
    let pool_type_hash: [u8; 32] = pool_type_script.calc_script_hash().unpack();

    let vault_lock_script = setup_vault_lock_script(
        &mut context,
        borrower_lock_hash,
        pool_type_hash,
        2_000_000_000,
    );

    let vault_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(vault_lock_script)
            .build(),
        Bytes::new(),
    );
    let borrower_auth_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(borrower_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Borrower auth input exists, but pool input is missing -> fail on pool requirement.
    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(vault_input)
                .build(),
        )
        .input(
            CellInput::new_builder()
                .previous_output(borrower_auth_input)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(borrower_lock)
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();

    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(err
        .to_string()
        .contains(&format!("error code {}", ERROR_WRONG_POOL_RECIPIENT)));
}

#[test]
fn test_vault_lock_rejects_public_unlock_before_expiry() {
    let mut context = Context::default();
    let always_success_out_point = deploy_always_success(&mut context);

    let borrower_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"borrower"))
        .expect("build borrower lock");
    let borrower_lock_hash: [u8; 32] = borrower_lock.calc_script_hash().unpack();

    let pool_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"pool-type"))
        .expect("build pool type script");
    let pool_type_hash: [u8; 32] = pool_type_script.calc_script_hash().unpack();

    let t_exp = 2_000_000_000u64;
    let vault_lock_script =
        setup_vault_lock_script(&mut context, borrower_lock_hash, pool_type_hash, t_exp);

    let vault_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(vault_lock_script)
            .build(),
        Bytes::new(),
    );
    let pool_owner_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"pool-owner"))
        .expect("build pool owner lock");
    let pool_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(pool_owner_lock)
            .type_(Some(pool_type_script).pack())
            .build(),
        Bytes::new(),
    );
    let liquidator_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"liquidator"))
        .expect("build third-party lock");
    let third_party_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(liquidator_lock)
            .build(),
        Bytes::new(),
    );

    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(vault_input)
                .build(),
        )
        .input(CellInput::new_builder().previous_output(pool_input).build())
        .input(
            CellInput::new_builder()
                .previous_output(third_party_input)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(borrower_lock)
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();
    let tx = add_header_dep(&mut context, tx, t_exp - 1);
    let tx = context.complete_tx(tx);

    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(err
        .to_string()
        .contains(&format!("error code {}", ERROR_VAULT_NOT_EXPIRED)));
}

#[test]
fn test_vault_lock_allows_public_liquidation_after_expiry() {
    let mut context = Context::default();
    let always_success_out_point = deploy_always_success(&mut context);

    let borrower_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"borrower"))
        .expect("build borrower lock");
    let borrower_lock_hash: [u8; 32] = borrower_lock.calc_script_hash().unpack();

    let pool_type_script = context
        .build_script(&always_success_out_point, Bytes::from_static(b"pool-type"))
        .expect("build pool type script");
    let pool_type_hash: [u8; 32] = pool_type_script.calc_script_hash().unpack();

    let t_exp = 2_000_000_000u64;
    let vault_lock_script =
        setup_vault_lock_script(&mut context, borrower_lock_hash, pool_type_hash, t_exp);

    let vault_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(vault_lock_script)
            .build(),
        Bytes::new(),
    );
    let pool_owner_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"pool-owner"))
        .expect("build pool owner lock");
    let pool_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(pool_owner_lock)
            .type_(Some(pool_type_script).pack())
            .build(),
        Bytes::new(),
    );
    let liquidator_lock = context
        .build_script(&always_success_out_point, Bytes::from_static(b"liquidator"))
        .expect("build third-party lock");
    let third_party_input = context.create_cell(
        CellOutput::new_builder()
            .capacity(10_000u64)
            .lock(liquidator_lock)
            .build(),
        Bytes::new(),
    );

    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(vault_input)
                .build(),
        )
        .input(CellInput::new_builder().previous_output(pool_input).build())
        .input(
            CellInput::new_builder()
                .previous_output(third_party_input)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(10_000u64)
                .lock(borrower_lock)
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();
    let tx = add_header_dep(&mut context, tx, t_exp + 1);
    let tx = context.complete_tx(tx);

    context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("third party should unlock after expiry with pool input present");
}
