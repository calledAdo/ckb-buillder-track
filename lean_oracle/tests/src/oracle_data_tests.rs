//! This file is the main executable documentation suite for `lean_oracle`.
//!
//! Every test here is intentionally explicit because the project depends on:
//! - byte-precise layouts
//! - exact hashing rules
//! - exact signature recovery behavior
//! - exact accumulator parsing behavior
//!
//! In practice, these tests serve as both validation and worked examples.

use k256::ecdsa::SigningKey;
use lean_oracle_common::{
    guardian_set::GuardianSetData,
    merkle::{keccak256, pyth_leaf_hash, verify_merkle_proof, verify_pyth_merkle_proof},
    oracle_data::OracleData,
    oracle_witness::OracleUpdateWitness,
    pyth_accumulator::{ParsedAccumulatorUpdate, ParsedAccumulatorUpdateForFeed, PriceFeedMessage},
    wormhole_vaa::ParsedVaa,
    wormhole_verify::{ethereum_address, verify_guardian_quorum},
};
use sha3::{Digest, Keccak256};

// Real Wormhole guardian set fetched from the currently active on-chain
// guardian registry for the environment the user is targeting.
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

// Real Hermes accumulator payload supplied by the user for BTC/USD testing.
const REAL_HERMES_ACCUMULATOR_HEX: &str = "504e41550100000003b801000000050d00198c483ca709c2ffd89a366b294ff22535e1c075185c752d29f48ce74dddb14c77617bdfc34a6e66d8a8ae5e649ed0d57da617a667a280092765238cab85d96a0103553197fcd6f4916bbcf186ac07adf77bd25b96d137222f0452e554529f83c41a1254d0dafb779aa9186b3ab7487e109b64f6ca8871b3ef660382c5ae5cba6bb601045d6bca6dd20dd35cdf0110f825a384d23e0806971c908fb97402a1cfbd7b58e859dcf8435a0f19cc912cb757db7f01f867ec3973db1a6a54902e98fecdb448e70006e6a724cfdf5089dfbcc239c2bb3d320bfcb376b57557edfd70b2a3084370044819ada602dc7c1b3b90ead345f2d29cead915c9bf16afc34ee870e34ce6d61e8e01070e62d6ac3210bd9fd41a1b773a4dbca309ad97c87da0f37196b971373a88e4dd35a0f03c10bb93c0e00b72b3e43678e71fb2f7b27c132694dde6d4085eacbc770008890eb9e89edb3c2cecea2e48288e2db8c179ee65404f51325a40d0d6b32031486281b8f33e94413f07e4c3d4860d6e3e17e23abe136384d5a3661c2000eef0a90009f18ab73b5f3d3f63af8ac8c4c474a0c05085242b5da51203dc410f5b7855e5b17aee0b11ad3d8e52b39bd56535f84455bbff2125e5ac803c98965192608d1d54010aa030713bddeb9dfff93f0d246d7cc23f602ab5a7411467a2c0284f8799700e30446e69152f8c8f17ba7608143e339e646544b16390b48920bdc6ce5f0fcb75c7010dbaf5e46bdc129992fa929d640a23e8c9f90616a603d4c891211dc3405b90f1f125b8e87d0f2976cbaee302c511dd852e646724f7c0e5cae4b791c3838fa5d6db000ecf1513667f288e62ee26588476ab0f71e27112a43f1731487a56afd64c6567e8769c30a30aeb5de22b54757baefa2739a8069757c1bbc3d670a056e924ddfeb5010fa4871af5e790c92cd44ea8af4414209d62e11bc434742d72b3ee4d8b6209926912def505e7bb7a1f3a1b24ec405017d0e3fdc2d0505811e9160fa1dd035e1a6f0010609d335bdff28da28dba1ba986fbb12b8619989b82f26e264586e3d6d60c7af839c3f4a30f3a36fc5b4385f6a5ee53b720222dbd83377be76b8695864ce012da0112649a1e89ce592801764b9cc63cd431be81e06dc3fb18b9b8ca22abf3d5b73eb44881bd97d56737f875cf09009a84d3291f747d25b6801e981491a483c995d88d0069d0197d00000000001ae101faedac5851e32b9b23b5f9411a8c2bac4aae3ed4dd7b811dd1a72ea4aa71000000000bcbc61d0141555756000000000010d9705000002710ad4ae92ea1f89b4fe4f3c15a1734e8f6e310c3a201005500e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43000006156e8a71720000000070425e77fffffff80000000069d0197d0000000069d0197c00000613f49bda80000000006a72ccce0d1419d929f2ad09cb3fe428d986e9a22482e384ef8ef5c6a87324b44c78ad5617b43c5172d8c49e2233abedaa70aa6532be7283780035d1b5ba6e0a1c341bb710feebc4551ff0fd594eab00af23295051894fc17a4c027bf85ae7c9ad4b525a1f6c947943c2cfb48d3ea42cfe9bbdb70bf4f08d8d15a838603e8fda00229eba03b06eae7c9e04c841a4168b863f524c7280dccdb2401949149af2f192035afebb91c22d0f2fb7aa97ce74cc6fdb833a78a76628ee95bc26f06a7f11604119a809f4b099b20e48ea07eb4cedfa7ac1f59641d52b936372f5059038fa7245a61c77620dc15cbbf727f815144e06593fdbd11ab677397c86a8d968eef04b21dc31e3c5c85ff9";

#[test]
fn test_oracle_data_roundtrip() {
    let original = OracleData {
        feed_id: [0x11; 32],
        guardian_set_type_hash: [0x99; 32],
        price: 123_456,
        conf: 789,
        expo: -7,
        publish_time: 1_775_182_293,
        prev_publish_time: 1_775_182_292,
        ema_price: 123_400,
        ema_conf: 700,
        guardian_set_index: 3,
        emitter_chain: 26,
        emitter_address: [0x22; 32],
    };

    let bytes = original.to_bytes();
    let decoded = OracleData::from_bytes(&bytes).expect("decode oracle state");
    assert_eq!(decoded, original);
}

#[test]
fn test_guardian_set_roundtrip() {
    let original = GuardianSetData {
        set_index: 42,
        quorum: 13,
        creation_time: 1_775_182_200,
        expiration_time: 0,
        governance_lock_hash: [0x33; 32],
        guardian_addresses: vec![[0x44; 20], [0x55; 20], [0x66; 20]],
    };

    let bytes = original.to_bytes();
    let decoded = GuardianSetData::from_bytes(&bytes).expect("decode guardian set");

    assert_eq!(decoded.set_index, original.set_index);
    assert_eq!(decoded.quorum, original.quorum);
    assert_eq!(decoded.creation_time, original.creation_time);
    assert_eq!(decoded.expiration_time, original.expiration_time);
    assert_eq!(decoded.governance_lock_hash, original.governance_lock_hash);
    assert_eq!(decoded.guardian_addresses, original.guardian_addresses);
}

#[test]
fn test_oracle_update_witness_roundtrip() {
    let original = OracleUpdateWitness {
        accumulator_update: vec![1, 2, 3, 4, 5],
    };

    let bytes = original.to_bytes();
    let decoded = OracleUpdateWitness::from_bytes(&bytes).expect("decode witness");
    assert_eq!(decoded, original);
}

#[test]
fn test_parse_minimal_wormhole_vaa() {
    let mut encoded = Vec::new();
    encoded.push(1);
    encoded.extend_from_slice(&3u32.to_be_bytes());
    encoded.push(2);

    encoded.push(1);
    encoded.extend_from_slice(&[0x11; 65]);
    encoded.push(4);
    encoded.extend_from_slice(&[0x22; 65]);

    encoded.extend_from_slice(&1_775_182_293u32.to_be_bytes());
    encoded.extend_from_slice(&7u32.to_be_bytes());
    encoded.extend_from_slice(&26u16.to_be_bytes());
    encoded.extend_from_slice(&[0x33; 32]);
    encoded.extend_from_slice(&99u64.to_be_bytes());
    encoded.push(1);
    encoded.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);

    let parsed = ParsedVaa::parse(&encoded).expect("parse vaa");
    assert_eq!(parsed.version, 1);
    assert_eq!(parsed.guardian_set_index, 3);
    assert_eq!(parsed.signatures.len(), 2);
    assert_eq!(parsed.signatures[0].guardian_index, 1);
    assert_eq!(parsed.signatures[0].v(), 0x11);
    assert_eq!(parsed.signatures[1].guardian_index, 4);
    assert_eq!(parsed.timestamp, 1_775_182_293);
    assert_eq!(parsed.nonce, 7);
    assert_eq!(parsed.emitter_chain, 26);
    assert_eq!(parsed.emitter_address, [0x33; 32]);
    assert_eq!(parsed.sequence, 99);
    assert_eq!(parsed.consistency_level, 1);
    assert_eq!(parsed.payload, vec![0xde, 0xad, 0xbe, 0xef]);
}

#[test]
fn test_verify_guardian_quorum_with_real_signatures() {
    let signing_key_1 = SigningKey::from_bytes((&[1u8; 32]).into()).expect("key1");
    let signing_key_2 = SigningKey::from_bytes((&[2u8; 32]).into()).expect("key2");

    let guardian_1 = ethereum_address(signing_key_1.verifying_key());
    let guardian_2 = ethereum_address(signing_key_2.verifying_key());

    let body = {
        let mut body = Vec::new();
        body.extend_from_slice(&1_775_182_293u32.to_be_bytes());
        body.extend_from_slice(&7u32.to_be_bytes());
        body.extend_from_slice(&26u16.to_be_bytes());
        body.extend_from_slice(&[0x33; 32]);
        body.extend_from_slice(&99u64.to_be_bytes());
        body.push(1);
        body.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        body
    };

    let inner = Keccak256::digest(&body);
    let digest_1 = Keccak256::new().chain_update(inner);
    let digest_2 = Keccak256::new().chain_update(Keccak256::digest(&body));

    let (sig1, recid1) = signing_key_1
        .sign_digest_recoverable(digest_1)
        .expect("sign1");
    let (sig2, recid2) = signing_key_2
        .sign_digest_recoverable(digest_2)
        .expect("sign2");

    let mut encoded = Vec::new();
    encoded.push(1);
    encoded.extend_from_slice(&3u32.to_be_bytes());
    encoded.push(2);
    encoded.push(0);
    encoded.extend_from_slice(&sig1.to_bytes());
    encoded.push(recid1.to_byte());
    encoded.push(1);
    encoded.extend_from_slice(&sig2.to_bytes());
    encoded.push(recid2.to_byte());
    encoded.extend_from_slice(&body);

    let vaa = ParsedVaa::parse(&encoded).expect("parse signed vaa");
    let guardian_set = GuardianSetData {
        set_index: 3,
        quorum: 2,
        creation_time: 1_775_182_200,
        expiration_time: 0,
        governance_lock_hash: [0x44; 32],
        guardian_addresses: vec![guardian_1, guardian_2],
    };

    assert!(verify_guardian_quorum(&vaa, &guardian_set).is_ok());
}

#[test]
fn test_verify_merkle_proof_for_two_leaf_tree() {
    let left = keccak256(b"left");
    let right = keccak256(b"right");

    let mut combined = [0u8; 64];
    combined[..32].copy_from_slice(&left);
    combined[32..].copy_from_slice(&right);
    let root = keccak256(&combined);

    assert!(verify_merkle_proof(root, left, &[right]));
    assert!(verify_merkle_proof(root, right, &[left]));
}

#[test]
fn test_verify_pyth_merkle_proof_for_single_leaf() {
    let message = b"example-price-message";
    let root = pyth_leaf_hash(message);
    assert!(verify_pyth_merkle_proof(root, message, &[]));
}

#[test]
fn test_parse_minimal_accumulator_update() {
    let signing_key_1 = SigningKey::from_bytes((&[1u8; 32]).into()).expect("key1");
    let signing_key_2 = SigningKey::from_bytes((&[2u8; 32]).into()).expect("key2");

    let price_message = PriceFeedMessage {
        feed_id: [0x77; 32],
        price: 1000,
        conf: 25,
        expo: -8,
        publish_time: 123456,
        prev_publish_time: 123455,
        ema_price: 995,
        ema_conf: 20,
    };

    let raw_message = encode_price_feed_message(&price_message);
    let root_digest = pyth_leaf_hash(&raw_message);

    let vaa_payload = {
        let mut payload = Vec::new();
        payload.extend_from_slice(&0x4155_5756u32.to_be_bytes());
        payload.push(0);
        payload.extend_from_slice(&282540738u64.to_be_bytes());
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.extend_from_slice(&root_digest);
        payload
    };

    let body = {
        let mut body = Vec::new();
        body.extend_from_slice(&1_775_182_293u32.to_be_bytes());
        body.extend_from_slice(&7u32.to_be_bytes());
        body.extend_from_slice(&26u16.to_be_bytes());
        body.extend_from_slice(&[0x33; 32]);
        body.extend_from_slice(&99u64.to_be_bytes());
        body.push(1);
        body.extend_from_slice(&vaa_payload);
        body
    };

    let inner = Keccak256::digest(&body);
    let (sig1, recid1) = signing_key_1
        .sign_digest_recoverable(Keccak256::new().chain_update(inner))
        .expect("sign1");
    let (sig2, recid2) = signing_key_2
        .sign_digest_recoverable(Keccak256::new().chain_update(Keccak256::digest(&body)))
        .expect("sign2");

    let vaa = {
        let mut encoded = Vec::new();
        encoded.push(1);
        encoded.extend_from_slice(&3u32.to_be_bytes());
        encoded.push(2);
        encoded.push(0);
        encoded.extend_from_slice(&sig1.to_bytes());
        encoded.push(recid1.to_byte());
        encoded.push(1);
        encoded.extend_from_slice(&sig2.to_bytes());
        encoded.push(recid2.to_byte());
        encoded.extend_from_slice(&body);
        encoded
    };

    let accumulator_update = {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(&0x504e_4155u32.to_be_bytes());
        encoded.push(1);
        encoded.push(0);
        encoded.push(0);
        encoded.push(0);
        encoded.extend_from_slice(&(vaa.len() as u16).to_be_bytes());
        encoded.extend_from_slice(&vaa);
        encoded.push(1);
        encoded.extend_from_slice(&(raw_message.len() as u16).to_be_bytes());
        encoded.extend_from_slice(&raw_message);
        encoded.push(0);
        encoded
    };

    let parsed = ParsedAccumulatorUpdate::parse(&accumulator_update).expect("parse accumulator");
    assert_eq!(parsed.vaa.guardian_set_index, 3);
    assert_eq!(parsed.vaa.emitter_chain, 26);
    assert_eq!(parsed.slot, 282540738);
    assert_eq!(parsed.root_digest, root_digest);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0], price_message);
}

#[test]
fn test_parse_accumulator_for_target_feed() {
    let signing_key_1 = SigningKey::from_bytes((&[1u8; 32]).into()).expect("key1");
    let signing_key_2 = SigningKey::from_bytes((&[2u8; 32]).into()).expect("key2");

    let target_message = PriceFeedMessage {
        feed_id: [0x77; 32],
        price: 1000,
        conf: 25,
        expo: -8,
        publish_time: 123456,
        prev_publish_time: 123455,
        ema_price: 995,
        ema_conf: 20,
    };
    let other_message = PriceFeedMessage {
        feed_id: [0x88; 32],
        price: 2000,
        conf: 30,
        expo: -8,
        publish_time: 223456,
        prev_publish_time: 223455,
        ema_price: 1995,
        ema_conf: 22,
    };

    let target_raw = encode_price_feed_message(&target_message);
    let other_raw = encode_price_feed_message(&other_message);

    let target_leaf = pyth_leaf_hash(&target_raw);
    let other_leaf = pyth_leaf_hash(&other_raw);
    let root_digest = lean_oracle_common::merkle::pyth_node_hash(target_leaf, other_leaf);

    let vaa_payload = {
        let mut payload = Vec::new();
        payload.extend_from_slice(&0x4155_5756u32.to_be_bytes());
        payload.push(0);
        payload.extend_from_slice(&282540738u64.to_be_bytes());
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.extend_from_slice(&root_digest);
        payload
    };

    let body = {
        let mut body = Vec::new();
        body.extend_from_slice(&1_775_182_293u32.to_be_bytes());
        body.extend_from_slice(&7u32.to_be_bytes());
        body.extend_from_slice(&26u16.to_be_bytes());
        body.extend_from_slice(&[0x33; 32]);
        body.extend_from_slice(&99u64.to_be_bytes());
        body.push(1);
        body.extend_from_slice(&vaa_payload);
        body
    };

    let inner = Keccak256::digest(&body);
    let (sig1, recid1) = signing_key_1
        .sign_digest_recoverable(Keccak256::new().chain_update(inner))
        .expect("sign1");
    let (sig2, recid2) = signing_key_2
        .sign_digest_recoverable(Keccak256::new().chain_update(Keccak256::digest(&body)))
        .expect("sign2");

    let vaa = {
        let mut encoded = Vec::new();
        encoded.push(1);
        encoded.extend_from_slice(&3u32.to_be_bytes());
        encoded.push(2);
        encoded.push(0);
        encoded.extend_from_slice(&sig1.to_bytes());
        encoded.push(recid1.to_byte());
        encoded.push(1);
        encoded.extend_from_slice(&sig2.to_bytes());
        encoded.push(recid2.to_byte());
        encoded.extend_from_slice(&body);
        encoded
    };

    let accumulator_update = {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(&0x504e_4155u32.to_be_bytes());
        encoded.push(1);
        encoded.push(0);
        encoded.push(0);
        encoded.push(0);
        encoded.extend_from_slice(&(vaa.len() as u16).to_be_bytes());
        encoded.extend_from_slice(&vaa);
        encoded.push(2);
        encoded.extend_from_slice(&(target_raw.len() as u16).to_be_bytes());
        encoded.extend_from_slice(&target_raw);
        encoded.push(1);
        encoded.extend_from_slice(&other_leaf);
        encoded.extend_from_slice(&(other_raw.len() as u16).to_be_bytes());
        encoded.extend_from_slice(&other_raw);
        encoded.push(1);
        encoded.extend_from_slice(&target_leaf);
        encoded
    };

    let parsed = ParsedAccumulatorUpdateForFeed::parse_for_feed(&accumulator_update, &[0x77; 32])
        .expect("parse target feed");
    assert_eq!(parsed.vaa.guardian_set_index, 3);
    assert_eq!(parsed.slot, 282540738);
    assert_eq!(parsed.root_digest, root_digest);
    assert_eq!(parsed.message, target_message);
}

#[test]
fn test_parse_real_hermes_blob_and_verify_real_guardian_set() {
    let accumulator_update = decode_hex(REAL_HERMES_ACCUMULATOR_HEX).expect("decode real hex");
    let target_feed_id =
        decode_hex_32("e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43")
            .expect("decode feed id");

    let parsed =
        ParsedAccumulatorUpdateForFeed::parse_for_feed(&accumulator_update, &target_feed_id)
            .expect("parse real accumulator");

    let guardian_set = GuardianSetData {
        set_index: parsed.vaa.guardian_set_index,
        quorum: 13,
        creation_time: 0,
        expiration_time: 0,
        governance_lock_hash: [0u8; 32],
        guardian_addresses: REAL_GUARDIAN_SET
            .iter()
            .map(|addr| decode_hex_20(addr).expect("decode guardian"))
            .collect(),
    };

    assert!(verify_guardian_quorum(&parsed.vaa, &guardian_set).is_ok());
    assert_eq!(parsed.message.feed_id, target_feed_id);
    assert_eq!(parsed.message.price, 6_689_118_646_642);
    assert_eq!(parsed.message.conf, 1_883_397_751);
    assert_eq!(parsed.message.expo, -8);
    assert_eq!(parsed.message.publish_time, 1_775_245_693);
    assert_eq!(parsed.message.prev_publish_time, 1_775_245_692);
    assert_eq!(parsed.message.ema_price, 6_682_778_000_000);
    assert_eq!(parsed.message.ema_conf, 1_785_908_430);
}

fn encode_price_feed_message(message: &PriceFeedMessage) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0);
    out.extend_from_slice(&message.feed_id);
    out.extend_from_slice(&message.price.to_be_bytes());
    out.extend_from_slice(&message.conf.to_be_bytes());
    out.extend_from_slice(&message.expo.to_be_bytes());
    out.extend_from_slice(&message.publish_time.to_be_bytes());
    out.extend_from_slice(&message.prev_publish_time.to_be_bytes());
    out.extend_from_slice(&message.ema_price.to_be_bytes());
    out.extend_from_slice(&message.ema_conf.to_be_bytes());
    out
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, ()> {
    if hex.len() % 2 != 0 {
        return Err(());
    }

    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let hi = decode_nibble(bytes[i]).ok_or(())?;
        let lo = decode_nibble(bytes[i + 1]).ok_or(())?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn decode_hex_20(hex: &str) -> Result<[u8; 20], ()> {
    let bytes = decode_hex(hex)?;
    if bytes.len() != 20 {
        return Err(());
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn decode_hex_32(hex: &str) -> Result<[u8; 32], ()> {
    let bytes = decode_hex(hex)?;
    if bytes.len() != 32 {
        return Err(());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn decode_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
