//! Integration tests for the `oracle_type` and `guardian_set_type` contracts.
//!
//! These tests move beyond host-only parser checks and build actual CKB
//! transactions using `ckb-testtool`.

use ckb_testtool::{
    ckb_types::{
        bytes::Bytes,
        core::TransactionBuilder,
        packed::{CellDep, CellInput, CellOutput},
        prelude::*,
    },
    context::Context,
};
use lean_oracle_common::{
    errors::*,
    guardian_set::GuardianSetData,
    oracle_data::OracleData,
    oracle_witness::OracleUpdateWitness,
};

const MAX_CYCLES: u64 = 100_000_000;

const REAL_GUARDIAN_SET: [&str; 19] = [
    "5893B5A76c3f739645648885bDCcC06cd70a3Cd3",
    "fF6CB952589BDE862c25Ef4392132fb9D4A42157",
    "114De8460193bdf3A2fCf81f86a09765F4762fD1",
    "107A0086b32d7A0977926A205131d8731D39cbEB",
    "8C82B2fd82FaeD2711d59AF0F2499D16e726f6b2",
    "11b39756C042441BE6D8650b69b54EbE715E2343",
    "938f104AEb5581293216ce97d771e0CB721221B1",
    "15e7cAF07C4e3DC8e7C469f92C8Cd88FB8005a20",
    "74a3bf913953D695260D88BC1aA25A4eeE363ef0",
    "000aC0076727b35FBea2dAc28fEE5cCB0fEA768e",
    "AF45Ced136b9D9e24903464AE889F5C8a723FC14",
    "f93124b7c738843CBB89E864c862c38cddCccF95",
    "D2CC37A4dc036a8D232b48f62cDD4731412f4890",
    "DA798F6896A3331F64b48c12D1D57Fd9cbe70811",
    "D1F64e26238811de5553C40f64af41eE1B6057Cc",
    "43ac8f567A31e7850Da532B361988Bfe0d3ae11b",
    "178e21ad2E77AE06711549CFBB1f9c7a9d8096e8",
    "5E1487F35515d02A92753504a8D75471b9f49EdB",
    "6FbEBc898F403E4773E95feB15E80C9A99c8348d",
];

const REAL_HERMES_ACCUMULATOR_HEX: &str = "504e41550100000003b801000000050d00198c483ca709c2ffd89a366b294ff22535e1c075185c752d29f48ce74dddb14c77617bdfc34a6e66d8a8ae5e649ed0d57da617a667a280092765238cab85d96a0103553197fcd6f4916bbcf186ac07adf77bd25b96d137222f0452e554529f83c41a1254d0dafb779aa9186b3ab7487e109b64f6ca8871b3ef660382c5ae5cba6bb601045d6bca6dd20dd35cdf0110f825a384d23e0806971c908fb97402a1cfbd7b58e859dcf8435a0f19cc912cb757db7f01f867ec3973db1a6a54902e98fecdb448e70006e6a724cfdf5089dfbcc239c2bb3d320bfcb376b57557edfd70b2a3084370044819ada602dc7c1b3b90ead345f2d29cead915c9bf16afc34ee870e34ce6d61e8e01070e62d6ac3210bd9fd41a1b773a4dbca309ad97c87da0f37196b971373a88e4dd35a0f03c10bb93c0e00b72b3e43678e71fb2f7b27c132694dde6d4085eacbc770008890eb9e89edb3c2cecea2e48288e2db8c179ee65404f51325a40d0d6b32031486281b8f33e94413f07e4c3d4860d6e3e17e23abe136384d5a3661c2000eef0a90009f18ab73b5f3d3f63af8ac8c4c474a0c05085242b5da51203dc410f5b7855e5b17aee0b11ad3d8e52b39bd56535f84455bbff2125e5ac803c98965192608d1d54010aa030713bddeb9dfff93f0d246d7cc23f602ab5a7411467a2c0284f8799700e30446e69152f8c8f17ba7608143e339e646544b16390b48920bdc6ce5f0fcb75c7010dbaf5e46bdc129992fa929d640a23e8c9f90616a603d4c891211dc3405b90f1f125b8e87d0f2976cbaee302c511dd852e646724f7c0e5cae4b791c3838fa5d6db000ecf1513667f288e62ee26588476ab0f71e27112a43f1731487a56afd64c6567e8769c30a30aeb5de22b54757baefa2739a8069757c1bbc3d670a056e924ddfeb5010fa4871af5e790c92cd44ea8af4414209d62e11bc434742d72b3ee4d8b6209926912def505e7bb7a1f3a1b24ec405017d0e3fdc2d0505811e9160fa1dd035e1a6f0010609d335bdff28da28dba1ba986fbb12b8619989b82f26e264586e3d6d60c7af839c3f4a30f3a36fc5b4385f6a5ee53b720222dbd83377be76b8695864ce012da0112649a1e89ce592801764b9cc63cd431be81e06dc3fb18b9b8ca22abf3d5b73eb44881bd97d56737f875cf09009a84d3291f747d25b6801e981491a483c995d88d0069d0197d00000000001ae101faedac5851e32b9b23b5f9411a8c2bac4aae3ed4dd7b811dd1a72ea4aa71000000000bcbc61d0141555756000000000010d9705000002710ad4ae92ea1f89b4fe4f3c15a1734e8f6e310c3a201005500e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43000006156e8a71720000000070425e77fffffff80000000069d0197d0000000069d0197c00000613f49bda80000000006a72ccce0d1419d929f2ad09cb3fe428d986e9a22482e384ef8ef5c6a87324b44c78ad5617b43c5172d8c49e2233abedaa70aa6532be7283780035d1b5ba6e0a1c341bb710feebc4551ff0fd594eab00af23295051894fc17a4c027bf85ae7c9ad4b525a1f6c947943c2cfb48d3ea42cfe9bbdb70bf4f08d8d15a838603e8fda00229eba03b06eae7c9e04c841a4168b863f524c7280dccdb2401949149af2f192035afebb91c22d0f2fb7aa97ce74cc6fdb833a78a76628ee95bc26f06a7f11604119a809f4b099b20e48ea07eb4cedfa7ac1f59641d52b936372f5059038fa7245a61c77620dc15cbbf727f815144e06593fdbd11ab677397c86a8d968eef04b21dc31e3c5c85ff9";

fn load_oracle_type(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    let bin = std::fs::read("../target/riscv64imac-unknown-none-elf/release/oracle_type")
        .expect("missing oracle_type binary; build with cargo build -p oracle_script --release --target riscv64imac-unknown-none-elf");
    context.deploy_cell(bin.into())
}

fn load_guardian_set_type(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    let bin = std::fs::read("../target/riscv64imac-unknown-none-elf/release/guardian_set_type")
        .expect("missing guardian_set_type binary; build with cargo build -p guardian_set_script --release --target riscv64imac-unknown-none-elf");
    context.deploy_cell(bin.into())
}

fn deploy_always_success(context: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    context.deploy_cell(Bytes::from(ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec()))
}

fn code_dep(op: &ckb_testtool::ckb_types::packed::OutPoint) -> CellDep {
    CellDep::new_builder()
        .out_point(op.clone())
        .dep_type(ckb_testtool::ckb_types::core::DepType::Code)
        .build()
}

#[test]
fn test_oracle_type_accepts_real_hermes_update_with_real_guardian_dep() {
    let mut context = Context::default();

    let oracle_type_op = load_oracle_type(&mut context);
    let guardian_set_type_op = load_guardian_set_type(&mut context);
    let always_success_op = deploy_always_success(&mut context);

    let feed_id = decode_hex_32(
        "e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43",
    );
    let emitter_address =
        decode_hex_32("e101faedac5851e32b9b23b5f9411a8c2bac4aae3ed4dd7b811dd1a72ea4aa71");
    let accumulator_update = decode_hex(REAL_HERMES_ACCUMULATOR_HEX);

    let dummy_lock = context
        .build_script(&always_success_op, Bytes::from_static(b"oracle-lock"))
        .expect("build oracle lock");
    let guardian_lock = context
        .build_script(&always_success_op, Bytes::from_static(b"guardian-lock"))
        .expect("build guardian lock");

    let guardian_set_type_script = context
        .build_script(&guardian_set_type_op, Bytes::new())
        .expect("build guardian set type");
    let guardian_set_type_hash: [u8; 32] = guardian_set_type_script.calc_script_hash().unpack();

    let guardian_set_data = GuardianSetData {
        set_index: 5,
        quorum: 13,
        creation_time: 0,
        expiration_time: 0,
        governance_lock_hash: [0x11; 32],
        guardian_addresses: REAL_GUARDIAN_SET.iter().map(|a| decode_hex_20(a)).collect(),
    };

    let guardian_dep_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(100_000_000_000u64)
            .lock(guardian_lock)
            .type_(Some(guardian_set_type_script).pack())
            .build(),
        Bytes::from(guardian_set_data.to_bytes()),
    );

    let oracle_type_script = context
        .build_script(&oracle_type_op, Bytes::from(feed_id.to_vec()))
        .expect("build oracle type");

    let old_oracle = OracleData {
        feed_id,
        guardian_set_type_hash,
        price: 6_680_000_000_000,
        conf: 1_700_000_000,
        expo: -8,
        publish_time: 1_775_245_600,
        prev_publish_time: 1_775_245_599,
        ema_price: 6_670_000_000_000,
        ema_conf: 1_600_000_000,
        guardian_set_index: 5,
        emitter_chain: 26,
        emitter_address,
    };
    let new_oracle = OracleData {
        feed_id,
        guardian_set_type_hash,
        price: 6_689_118_646_642,
        conf: 1_883_397_751,
        expo: -8,
        publish_time: 1_775_245_693,
        prev_publish_time: 1_775_245_692,
        ema_price: 6_682_778_000_000,
        ema_conf: 1_785_908_430,
        guardian_set_index: 5,
        emitter_chain: 26,
        emitter_address,
    };

    let oracle_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(100_000_000_000u64)
            .lock(dummy_lock.clone())
            .type_(Some(oracle_type_script.clone()).pack())
            .build(),
        Bytes::from(old_oracle.to_bytes()),
    );

    let witness = OracleUpdateWitness { accumulator_update };
    let witness_args = ckb_testtool::ckb_types::packed::WitnessArgs::new_builder()
        .lock(Some(Bytes::from(witness.to_bytes())).pack())
        .build();

    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(oracle_input_out_point)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(100_000_000_000u64)
                .lock(dummy_lock)
                .type_(Some(oracle_type_script).pack())
                .build(),
        )
        .output_data(Bytes::from(new_oracle.to_bytes()).pack())
        .cell_dep(code_dep(&oracle_type_op))
        .cell_dep(code_dep(&guardian_set_type_op))
        .cell_dep(
            CellDep::new_builder()
                .out_point(guardian_dep_out_point)
                .dep_type(ckb_testtool::ckb_types::core::DepType::Code)
                .build(),
        )
        .witness(witness_args.as_bytes().pack())
        .build();

    let tx = context.complete_tx(tx);
    context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("real oracle update should verify");
}

#[test]
fn test_oracle_type_rejects_wrong_guardian_set_index() {
    let (context, tx) = build_real_oracle_update_tx(
        |guardian_set| guardian_set.set_index = 4,
        |_| {},
        |_| {},
    );

    let err = context
        .verify_tx(&tx, MAX_CYCLES)
        .expect_err("wrong guardian-set index should fail");
    assert!(err
        .to_string()
        .contains(&format!("error code {}", ERROR_GUARDIAN_SET_INDEX_MISMATCH)));
}

#[test]
fn test_oracle_type_rejects_wrong_emitter_address() {
    let (context, tx) = build_real_oracle_update_tx(
        |_| {},
        |new_oracle| new_oracle.emitter_address = [0x55; 32],
        |_| {},
    );

    let err = context
        .verify_tx(&tx, MAX_CYCLES)
        .expect_err("wrong emitter address should fail");
    assert!(err
        .to_string()
        .contains(&format!("error code {}", ERROR_CONFIG_MUTATED)));
}

#[test]
fn test_oracle_type_rejects_non_monotonic_publish_time() {
    let (context, tx) = build_real_oracle_update_tx(
        |_| {},
        |new_oracle| new_oracle.publish_time = 1_775_245_600,
        |old_oracle| old_oracle.publish_time = 1_775_245_600,
    );

    let err = context
        .verify_tx(&tx, MAX_CYCLES)
        .expect_err("non-monotonic publish time should fail");
    assert!(err
        .to_string()
        .contains(&format!("error code {}", ERROR_TIMESTAMP_NOT_MONOTONIC)));
}

#[test]
fn test_oracle_type_rejects_tampered_output_price() {
    let (context, tx) = build_real_oracle_update_tx(
        |_| {},
        |new_oracle| new_oracle.price += 1,
        |_| {},
    );

    let err = context
        .verify_tx(&tx, MAX_CYCLES)
        .expect_err("tampered oracle output should fail");
    assert!(err
        .to_string()
        .contains(&format!("error code {}", ERROR_PRICE_UPDATE_MISMATCH)));
}

fn build_real_oracle_update_tx<F, G, H>(
    mutate_guardian_set: F,
    mutate_new_oracle: G,
    mutate_old_oracle: H,
) -> (Context, ckb_testtool::ckb_types::core::TransactionView)
where
    F: FnOnce(&mut GuardianSetData),
    G: FnOnce(&mut OracleData),
    H: FnOnce(&mut OracleData),
{
    let mut context = Context::default();

    let oracle_type_op = load_oracle_type(&mut context);
    let guardian_set_type_op = load_guardian_set_type(&mut context);
    let always_success_op = deploy_always_success(&mut context);

    let feed_id = decode_hex_32(
        "e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43",
    );
    let emitter_address =
        decode_hex_32("e101faedac5851e32b9b23b5f9411a8c2bac4aae3ed4dd7b811dd1a72ea4aa71");
    let accumulator_update = decode_hex(REAL_HERMES_ACCUMULATOR_HEX);

    let dummy_lock = context
        .build_script(&always_success_op, Bytes::from_static(b"oracle-lock"))
        .expect("build oracle lock");
    let guardian_lock = context
        .build_script(&always_success_op, Bytes::from_static(b"guardian-lock"))
        .expect("build guardian lock");

    let guardian_set_type_script = context
        .build_script(&guardian_set_type_op, Bytes::new())
        .expect("build guardian set type");
    let guardian_set_type_hash: [u8; 32] = guardian_set_type_script.calc_script_hash().unpack();

    let mut guardian_set_data = GuardianSetData {
        set_index: 5,
        quorum: 13,
        creation_time: 0,
        expiration_time: 0,
        governance_lock_hash: [0x11; 32],
        guardian_addresses: REAL_GUARDIAN_SET.iter().map(|a| decode_hex_20(a)).collect(),
    };
    mutate_guardian_set(&mut guardian_set_data);

    let guardian_dep_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(100_000_000_000u64)
            .lock(guardian_lock)
            .type_(Some(guardian_set_type_script).pack())
            .build(),
        Bytes::from(guardian_set_data.to_bytes()),
    );

    let oracle_type_script = context
        .build_script(&oracle_type_op, Bytes::from(feed_id.to_vec()))
        .expect("build oracle type");

    let mut old_oracle = OracleData {
        feed_id,
        guardian_set_type_hash,
        price: 6_680_000_000_000,
        conf: 1_700_000_000,
        expo: -8,
        publish_time: 1_775_245_600,
        prev_publish_time: 1_775_245_599,
        ema_price: 6_670_000_000_000,
        ema_conf: 1_600_000_000,
        guardian_set_index: 5,
        emitter_chain: 26,
        emitter_address,
    };
    mutate_old_oracle(&mut old_oracle);

    let mut new_oracle = OracleData {
        feed_id,
        guardian_set_type_hash,
        price: 6_689_118_646_642,
        conf: 1_883_397_751,
        expo: -8,
        publish_time: 1_775_245_693,
        prev_publish_time: 1_775_245_692,
        ema_price: 6_682_778_000_000,
        ema_conf: 1_785_908_430,
        guardian_set_index: 5,
        emitter_chain: 26,
        emitter_address,
    };
    mutate_new_oracle(&mut new_oracle);

    let oracle_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(100_000_000_000u64)
            .lock(dummy_lock.clone())
            .type_(Some(oracle_type_script.clone()).pack())
            .build(),
        Bytes::from(old_oracle.to_bytes()),
    );

    let witness = OracleUpdateWitness { accumulator_update };
    let witness_args = ckb_testtool::ckb_types::packed::WitnessArgs::new_builder()
        .lock(Some(Bytes::from(witness.to_bytes())).pack())
        .build();

    let tx = TransactionBuilder::default()
        .input(
            CellInput::new_builder()
                .previous_output(oracle_input_out_point)
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .capacity(100_000_000_000u64)
                .lock(dummy_lock)
                .type_(Some(oracle_type_script).pack())
                .build(),
        )
        .output_data(Bytes::from(new_oracle.to_bytes()).pack())
        .cell_dep(code_dep(&oracle_type_op))
        .cell_dep(code_dep(&guardian_set_type_op))
        .cell_dep(
            CellDep::new_builder()
                .out_point(guardian_dep_out_point)
                .dep_type(ckb_testtool::ckb_types::core::DepType::Code)
                .build(),
        )
        .witness(witness_args.as_bytes().pack())
        .build();

    let tx = context.complete_tx(tx);
    (context, tx)
}

fn decode_hex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "hex length must be even");
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let hi = decode_nibble(bytes[i]);
        let lo = decode_nibble(bytes[i + 1]);
        out.push((hi << 4) | lo);
        i += 2;
    }
    out
}

fn decode_hex_20(hex: &str) -> [u8; 20] {
    let bytes = decode_hex(hex);
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    out
}

fn decode_hex_32(hex: &str) -> [u8; 32] {
    let bytes = decode_hex(hex);
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

fn decode_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => panic!("invalid hex byte"),
    }
}
