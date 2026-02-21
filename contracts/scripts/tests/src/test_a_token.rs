use ckb_testtool::{
    ckb_types::{
        bytes::Bytes,
        core::TransactionBuilder,
        packed::*,
        prelude::*,
    },
    context::Context,
};

use blake2b_ref::Blake2bBuilder;

// Errors based on scripts
const ERROR_INSUFFICIENT_BALANCE: i8 = 52;
const ERROR_DATA_MALFORMED: i8 = 51;
const ERROR_ALLOWANCE_NOT_BURNED: i8 = 53;
const ERROR_FORGED_VAULT_ID: i8 = 54;
const ERROR_REFUND_MISSING: i8 = 55;
const ERROR_SIGNATURE_INVALID: i8 = -3;
const ERROR_ALLOWANCE_NOT_FOUND: i8 = 50;

// Max Cycles required for normal execution
const MAX_CYCLES: u64 = 15_000_000;

fn deploy_scripts(context: &mut Context) -> (OutPoint, OutPoint) {
    let type_bin = std::fs::read("../../target/riscv64imac-unknown-none-elf/release/a_udt_type").expect("read a_udt_type binaries");
    let type_out_point = context.deploy_cell(type_bin.into());

    let lock_bin = std::fs::read("../../target/riscv64imac-unknown-none-elf/release/a_udt_lock").expect("read a_udt_lock binaries");
    let lock_out_point = context.deploy_cell(lock_bin.into());

    (type_out_point, lock_out_point)
}

fn build_normal_data(amount: u128, owner_pubkey: Option<&[u8]>) -> Bytes {
    let mut data = vec![0u8; 69]; // Standard size for a_token normal cells
    data[0] = 0; // variant normal
    data[1..17].copy_from_slice(&amount.to_le_bytes());
    if let Some(_pubkey) = owner_pubkey {
       // fill the rest or ignore since type script only cares about first 17 bytes for normal tokens
    }
    Bytes::from(data)
}

fn build_vault_data(amount: u128, vault_id: &[u8; 32], owner_lock: &[u8; 20]) -> Bytes {
    let mut data = vec![0u8; 69];
    data[0] = 1; // variant vault (allowance)
    data[1..17].copy_from_slice(&amount.to_le_bytes());
    data[17..49].copy_from_slice(vault_id);
    data[49..69].copy_from_slice(owner_lock);
    Bytes::from(data)
}

fn calculate_expected_vault_id(first_input: &OutPoint) -> [u8; 32] {
    let mut blake2b = Blake2bBuilder::new(32).personal(b"ckb-default-hash").build();
    blake2b.update(first_input.as_slice());
    let mut expected_vault_id = [0u8; 32];
    blake2b.finalize(&mut expected_vault_id);
    expected_vault_id
}

/// **Test:** `test_type_owner_bypass_mint`
///
/// **What we are testing for:**
/// The ability of the central issuer (owner) to mint tokens freely without being subjected to the conservation of balance rule (`input_tokens >= output_tokens`).
///
/// **How the test approaches it:**
/// 1. Deploys the A_Token Type script.
/// 2. Deploys an Always Success Lock script to represent the owner.
/// 3. Computes the hash of the owner's Lock script and passes it as arguments to the token's Type script (making the script recognize this lock as the issuer).
/// 4. Constructs a transaction where the input has 0 tokens and the output has 100 tokens, signed/unlocked by the owner.
///
/// **What to expect:**
/// The transaction completes successfully. The Type script detects the owner's lock in the inputs and returns 0 (success) immediately, bypassing all other vault/balance validations.
#[test]
fn test_type_owner_bypass_mint() {
    let mut context = Context::default();
    let (type_out_point, _) = deploy_scripts(&mut context);

    // Setup an owner lock script (always deployed to simulate owner lock logic)
    let owner_lock_bin = Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec());
    let owner_lock_out_point = context.deploy_cell(owner_lock_bin);

    let owner_lock_script = context
        .build_script(&owner_lock_out_point, Bytes::from("owner_args"))
        .expect("script");
    let owner_lock_hash = owner_lock_script.calc_script_hash();

    // Type script requires lock hash of issuer in args
    let mut type_args = vec![0u8; 20];
    type_args.copy_from_slice(&owner_lock_hash.as_bytes()[0..20]);
    
    let token_type_script = context
        .build_script(&type_out_point, Bytes::from(type_args))
        .expect("script");

    // Input: 0 tokens, Output: 100 tokens (Minting)
    let capacity = 1000u64;
    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64)
            .lock(owner_lock_script.clone())
            .build(),
        Bytes::new(),
    );
    let input = CellInput::new_builder().previous_output(input_out_point).build();

    let output = CellOutput::new_builder()
        .capacity(capacity.clone())
        .lock(owner_lock_script.clone())
        .type_(Some(token_type_script).pack())
        .build();
    let output_data = build_normal_data(100, None);

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(output_data.pack())
        .build();

    let tx = context.complete_tx(tx);
    let cycles = context.verify_tx(&tx, MAX_CYCLES).expect("owner bypass should mint without balance check");
    println!("test_type_owner_bypass_mint consumes cycles: {}", cycles);
}

/// **Test:** `test_type_conservation_of_balance_success`
///
/// **What we are testing for:**
/// Standard token transfers between non-issuers, ensuring that tokens cannot be "printed" out of thin air by regular users.
///
/// **How the test approaches it:**
/// 1. Sets up the environment with a user lock script.
/// 2. Configures the token Type script with an arbitrary (random) issuer lock hash, ensuring the test runs in "User Mode".
/// 3. Provides exactly 100 tokens as input.
/// 4. Generates exactly 100 tokens as output.
///
/// **What to expect:**
/// The test will verify successfully since total `input_amount >= output_amount`, adhering to the conservation of balance rule.
#[test]
fn test_type_conservation_of_balance_success() {
    let mut context = Context::default();
    let (type_out_point, _) = deploy_scripts(&mut context);

    let dummy_lock_out_point = context.deploy_cell(Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec()));
    let dummy_lock_script = context
        .build_script(&dummy_lock_out_point, Bytes::from("user1"))
        .expect("script");

    let token_type_script = context
        .build_script(&type_out_point, Bytes::from(vec![0u8; 20])) // Issuer args doesn't match
        .expect("script");

    let capacity = 1000u64;
    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64)
            .lock(dummy_lock_script.clone())
            .type_(Some(token_type_script.clone()).pack())
            .build(),
        build_normal_data(100, None),
    );
    let input = CellInput::new_builder().previous_output(input_out_point).build();

    let output = CellOutput::new_builder()
        .capacity(capacity.clone())
        .lock(dummy_lock_script.clone())
        .type_(Some(token_type_script.clone()).pack())
        .build();
    let output_data = build_normal_data(100, None); // Output = Input

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(output_data.pack())
        .build();

    let tx = context.complete_tx(tx);
    let cycles = context.verify_tx(&tx, MAX_CYCLES).expect("balance preserved");
    println!("test_type_conservation_of_balance_success consumes cycles: {}", cycles);
}

/// **Test:** `test_type_conservation_of_balance_failure`
///
/// **What we are testing for:**
/// The prevention of unauthorized token minting (inflation) by regular users.
///
/// **How the test approaches it:**
/// 1. Simulates a standard user transfer scenario (User Mode).
/// 2. Provides 50 tokens as input but attempts to create output cells carrying 100 tokens.
///
/// **What to expect:**
/// The transaction fails verification. The Type script calculates that `input_amount < output_amount` and aborts with `ERROR_INSUFFICIENT_BALANCE` (error code 52).
#[test]
fn test_type_conservation_of_balance_failure() {
    let mut context = Context::default();
    let (type_out_point, _) = deploy_scripts(&mut context);

    let dummy_lock_out_point = context.deploy_cell(Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec()));
    let dummy_lock_script = context
        .build_script(&dummy_lock_out_point, Bytes::from("user1"))
        .expect("script");

    let token_type_script = context.build_script(&type_out_point, Bytes::from(vec![0u8; 20])).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64)
            .lock(dummy_lock_script.clone())
            .type_(Some(token_type_script.clone()).pack())
            .build(),
        build_normal_data(50, None), // Input 50
    );
    
    let output = CellOutput::new_builder()
        .capacity(1000u64)
        .lock(dummy_lock_script.clone())
        .type_(Some(token_type_script.clone()).pack())
        .build();
    let output_data = build_normal_data(100, None); // Output 100

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .output(output)
        .output_data(output_data.pack())
        .build();

    let tx = context.complete_tx(tx);
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(err.to_string().contains(&format!("error code {}", ERROR_INSUFFICIENT_BALANCE)));
}

/// **Test:** `test_type_create_vault`
///
/// **What we are testing for:**
/// The secure generation of a new "Vault" (Allowance) cell and ensuring it gets properly bound to a unique Vault ID.
///
/// **How the test approaches it:**
/// 1. Takes 100 normal tokens as input.
/// 2. Splits the output into two cells: 50 normal tokens and 50 tokens structured as a Vault (`variant = 1`).
/// 3. Calculates the expected Vault ID locally by hashing the first `CellInput` OutPoint (using Blake2b), replicating the Type script's internal logic.
/// 4. Assigns the calculated expected Vault ID to the new Vault output cell.
///
/// **What to expect:**
/// The test verifies successfully. The Type script sees higher allowance outputs than inputs (0 -> 1), realizes a vault is being created, recalculates the Vault ID from the inputs, and verifies that the output Vault cell uses this exact ID to prevent replay attacks and forgery.
#[test]
fn test_type_create_vault() {
    let mut context = Context::default();
    let (type_out_point, _) = deploy_scripts(&mut context);

    let dummy_lock_out_point = context.deploy_cell(Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec()));
    let dummy_lock = context.build_script(&dummy_lock_out_point, Bytes::from("user1")).unwrap();
    let type_script = context.build_script(&type_out_point, Bytes::from(vec![0u8; 20])).unwrap();

    let input_out_point = context.create_cell(
        CellOutput::new_builder().capacity(1000u64).lock(dummy_lock.clone()).type_(Some(type_script.clone()).pack()).build(),
        build_normal_data(100, None),
    );
    let input = CellInput::new_builder().previous_output(input_out_point.clone()).build();

    let expected_vault_id = calculate_expected_vault_id(&input_out_point);
    let owner_lock_hash = [1u8; 20];

    // Splitting 100 tokens into 50 normal and 50 vault
    let out_normal = CellOutput::new_builder().capacity(500u64).lock(dummy_lock.clone()).type_(Some(type_script.clone()).pack()).build();
    let out_vault = CellOutput::new_builder().capacity(500u64).lock(dummy_lock.clone()).type_(Some(type_script.clone()).pack()).build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(out_normal)
        .output(out_vault)
        .output_data(build_normal_data(50, None).pack())
        .output_data(build_vault_data(50, &expected_vault_id, &owner_lock_hash).pack())
        .build();

    let tx = context.complete_tx(tx);
    let cycles = context.verify_tx(&tx, MAX_CYCLES).unwrap();
    println!("test_type_create_vault consumes cycles: {}", cycles);
}

/// **Test:** `test_type_spend_vault_with_refund`
///
/// **What we are testing for:**
/// The execution rules when a delegate spends a Vault (Allowance), specifically ensuring that the Vault cell is fully consumed and the underlying CKB capacity rent is refunded back to the Vault's original owner.
///
/// **How the test approaches it:**
/// 1. Constructs an input Vault cell containing 100 delegated tokens. The Vault data stores the `pubkey_hash` of its owner (`expected_owner_20`).
/// 2. In the outputs, the Vault is completely converted back to 100 normal tokens (the partial spending rule prevents keeping partial allowances).
/// 3. Creates a separate "refund" output cell holding 1000 CKB capacity, locked specifically to the `expected_owner_20` pubkey hash extracted from the Input Vault cell.
///
/// **What to expect:**
/// The transaction passes successfully. The Type Script successfully detects `input_allowances > 0` and `output_allowances == 0`, confirms the Vault is burned, and scans the outputs to guarantee the 1000 CKB rent capacity was indeed returned to the original owner's lock script.
#[test]
fn test_type_spend_vault_with_refund() {
    let mut context = Context::default();
    let (type_out_point, _) = deploy_scripts(&mut context);

    let dummy_lock_out_point = context.deploy_cell(Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec()));
    let dummy_lock = context.build_script(&dummy_lock_out_point, Bytes::from("user1")).unwrap();
    let type_script = context.build_script(&type_out_point, Bytes::from(vec![0u8; 20])).unwrap();

    let expected_owner_hash = dummy_lock.calc_script_hash();
    let mut expected_owner_20 = [0u8; 20];
    expected_owner_20.copy_from_slice(&expected_owner_hash.as_bytes()[0..20]);

    let input_out_point = context.create_cell(
        CellOutput::new_builder().capacity(1000u64).lock(dummy_lock.clone()).type_(Some(type_script.clone()).pack()).build(),
        build_vault_data(100, &[0u8; 32], &expected_owner_20),
    );
    let input = CellInput::new_builder().previous_output(input_out_point).build();

    // The output should be 100 normal tokens, vault is consumed, but we MUST refund 1000 CKB to expected_owner_20
    let out_normal = CellOutput::new_builder().capacity(500u64).lock(dummy_lock.clone()).type_(Some(type_script.clone()).pack()).build();
    let refund_output = CellOutput::new_builder().capacity(1000u64).lock(dummy_lock.clone()).build(); // dummy_lock's hash matches expected_owner_20

    let tx = TransactionBuilder::default()
        .input(input)
        .output(out_normal)
        .output(refund_output)
        .output_data(build_normal_data(100, None).pack())
        .output_data(Bytes::new().pack())
        .build();

    let tx = context.complete_tx(tx);
    let cycles = context.verify_tx(&tx, MAX_CYCLES).unwrap();
    println!("test_type_spend_vault_with_refund consumes cycles: {}", cycles);
}

/// **Test:** `test_lock_allowance_fallback_success`
///
/// **What we are testing for:**
/// The `a_udt_lock` script's ability to act as a dual-strategy lock. When a cryptographic signature fails, it falls back to checking the inputs for a valid, Type-Script-approved Allowance cell.
///
/// **How the test approaches it:**
/// 1. Deploys the custom `a_udt_lock` script and assigns it the wrong expected pubkey hash, ensuring that standard signature validation (`ckb_auth`) will deliberately fail.
/// 2. Constructs Input 0: The "Normal Token" currently secured by this lock script. We embed a unique `target_token_id` in its payload.
/// 3. Constructs Input 1: An "Allowance Token" (Vault) issued by the same Type script, embedding the identical `target_token_id`.
/// 4. Submits the transaction for verification.
///
/// **What to expect:**
/// Despite missing the correct signature, the test passes. `a_udt_lock` signature check fails, so it reads the `target_token_id` from Input 0, scans the remaining inputs, locates Input 1 (which matches the required Token ID and has the identical root-of-trust Type Hash), and accepts the transaction as "delegated".
#[test]
fn test_lock_allowance_fallback_success() {
    let mut context = Context::default();
    let (type_out_point, lock_out_point) = deploy_scripts(&mut context);

    let udt_type_script = context.build_script(&type_out_point, Bytes::from(vec![0u8; 20])).unwrap();
    let _udt_type_hash = udt_type_script.calc_script_hash();

    // The token protected by a_udt_lock has target_token_id (e.g. [8u8; 32])
    let target_token_id = [8u8; 32];
    
    // Deploy Lock script with WRONG expected pubkey hash so signature check fails!
    let my_lock_script = context.build_script(&lock_out_point, Bytes::from(vec![9u8; 20])).unwrap();

    // Input 0: The normal token we're trying to move
    let token_output = CellOutput::new_builder().capacity(500u64).lock(my_lock_script.clone()).type_(Some(udt_type_script.clone()).pack()).build();
    
    // Simulate token cell data carrying the token ID in bytes 17..49 (a_token lock expects this structure for normal tokens falling back)
    let mut token_data = vec![0u8; 69];
    token_data[17..49].copy_from_slice(&target_token_id);
    let token_out_point = context.create_cell(token_output, Bytes::from(token_data.clone()));

    let allowance_owner_lock = context.build_script(&lock_out_point, Bytes::from(vec![1u8; 20])).unwrap();
    let expected_owner_hash = allowance_owner_lock.calc_script_hash();
    let mut expected_owner_20 = [0u8; 20];
    expected_owner_20.copy_from_slice(&expected_owner_hash.as_bytes()[0..20]);

    // Input 1: The Allowance cell stored somewhere else in the inputs, valid because it has the exact type hash and references the token_id
    let allowance_output = CellOutput::new_builder().capacity(1000u64).lock(allowance_owner_lock.clone()).type_(Some(udt_type_script.clone()).pack()).build();
    let allowance_data = build_vault_data(50, &target_token_id, &expected_owner_20); // ref_id is at bytes 17..49 for variant=1
    let allowance_out_point = context.create_cell(allowance_output, allowance_data);

    // Provide the 1000 CKB Refund back to the allowance owner so the Type Script doesn't fail with 55
    let refund_output = CellOutput::new_builder().capacity(1000u64).lock(allowance_owner_lock.clone()).build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(token_out_point).build())
        .input(CellInput::new_builder().previous_output(allowance_out_point).build())
        .output(CellOutput::new_builder().capacity(500u64).lock(my_lock_script.clone()).build())
        .output(refund_output)
        .output_data(Bytes::new().pack())
        .output_data(Bytes::new().pack())
        .build();

    let tx = context.complete_tx(tx);
    
    // The lock should pass because Input 1 provides an Allowance fallback for Input 0
    let cycles = context.verify_tx(&tx, MAX_CYCLES).expect("allowance fallback logic passed");
    println!("test_lock_allowance_fallback_success consumes cycles: {}", cycles);
}

/// **Test:** `test_lock_allowance_fallback_failure`
///
/// **What we are testing for:**
/// The correct rejection of a transaction by the `a_udt_lock` script when both the signature is invalid *and* no valid Allowance cell is provided in the inputs.
///
/// **How the test approaches it:**
/// 1. Sets up the same deliberately failing signature scenario as the previous test.
/// 2. Only provides the "Normal Token" as an input cell.
/// 3. We completely omit the "Allowance Token" (Vault) cell from the inputs, leaving the transaction without a valid delegate permission slip.
///
/// **What to expect:**
/// The transaction fails. The lock script first attempts `ckb_auth` signature validation (fails), then falls back to `check_allowance_in_inputs` (fails because no matching cell exists), ultimately rejecting the transaction with `ERROR_SIGNATURE_INVALID` or `ERROR_ALLOWANCE_NOT_FOUND`.
#[test]
fn test_lock_allowance_fallback_failure() {
    let mut context = Context::default();
    let (type_out_point, lock_out_point) = deploy_scripts(&mut context);

    let udt_type_script = context.build_script(&type_out_point, Bytes::from(vec![0u8; 20])).unwrap();
    let target_token_id = [8u8; 32];
    
    let my_lock_script = context.build_script(&lock_out_point, Bytes::from(vec![9u8; 20])).unwrap();

    let token_output = CellOutput::new_builder().capacity(500u64).lock(my_lock_script.clone()).type_(Some(udt_type_script.clone()).pack()).build();
    let mut token_data = vec![0u8; 69];
    token_data[17..49].copy_from_slice(&target_token_id);
    let token_out_point = context.create_cell(token_output, Bytes::from(token_data.clone()));

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(token_out_point).build())
        .output(CellOutput::new_builder().capacity(500u64).lock(my_lock_script.clone()).build())
        .output_data(Bytes::new().pack()) 
        .build();

    let tx = context.complete_tx(tx);
    
    // Fails since ckb_auth fail AND allowance fallback fails
    let err = context.verify_tx(&tx, MAX_CYCLES).unwrap_err();
    assert!(err.to_string().contains(&format!("error code {}", ERROR_SIGNATURE_INVALID)) || err.to_string().contains(&format!("error code {}", ERROR_ALLOWANCE_NOT_FOUND)));
}
