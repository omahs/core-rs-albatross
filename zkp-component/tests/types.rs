use std::path::PathBuf;

use ark_groth16::Proof;
use nimiq_block::MacroBlock;
use nimiq_database_value::{AsDatabaseBytes, FromDatabaseValue};
use nimiq_test_utils::zkp_test_data::ZKP_TEST_KEYS_PATH;
use nimiq_zkp_component::types::{ProofInput, ZKPState, ZKProof};

#[test]
fn it_serializes_and_deserializes_zk_proof() {
    let b = ZKProof {
        block_number: 0,
        proof: None,
    };
    let serialized = postcard::to_allocvec(&b).unwrap();
    let deserialized: ZKProof = postcard::from_bytes(&serialized).unwrap();
    assert_eq!(deserialized, b);

    let proof = ZKProof {
        block_number: 0,
        proof: Some(Proof::default()),
    };
    let serialized = postcard::to_allocvec(&proof).unwrap();
    let deserialized: ZKProof = postcard::from_bytes(&serialized).unwrap();
    assert_eq!(deserialized, proof);
}

#[test]
fn it_serializes_and_deserializes_to_bytes_zk_proof() {
    let proof = ZKProof {
        block_number: 0,
        proof: None,
    };
    let serialized = proof.as_database_bytes();
    let deserialized: ZKProof = FromDatabaseValue::copy_from_database(&serialized).unwrap();
    assert_eq!(deserialized, proof);

    let proof = ZKProof {
        block_number: 0,
        proof: Some(Proof::default()),
    };
    let serialized = proof.as_database_bytes();
    let deserialized: ZKProof = FromDatabaseValue::copy_from_database(&serialized).unwrap();
    assert_eq!(deserialized, proof);
}

#[test]
fn it_serializes_and_deserializes_zkp_state() {
    let state = ZKPState {
        latest_block: MacroBlock::default(),
        latest_proof: Some(Proof::default()),
    };
    let serialized = postcard::to_allocvec(&state).unwrap();
    let deserialized: ZKPState = postcard::from_bytes(&serialized).unwrap();
    assert_eq!(deserialized, state);

    let state = ZKPState {
        latest_block: MacroBlock::default(),
        latest_proof: None,
    };
    let serialized = postcard::to_allocvec(&state).unwrap();
    let deserialized: ZKPState = postcard::from_bytes(&serialized).unwrap();
    assert_eq!(deserialized, state);
}

#[test]
fn it_serializes_and_deserializes_proof_input() {
    let proof_input = ProofInput {
        previous_block: MacroBlock::default(),
        previous_proof: Some(Proof::default()),
        final_block: MacroBlock::default(),
        genesis_header_hash: [2; 32],
        prover_keys_path: PathBuf::from(ZKP_TEST_KEYS_PATH),
    };
    let serialized = postcard::to_allocvec(&proof_input).unwrap();
    let deserialized: ProofInput = postcard::from_bytes(&serialized).unwrap();
    assert_eq!(deserialized, proof_input);

    let proof_input = ProofInput {
        previous_block: MacroBlock::default(),
        previous_proof: None,
        final_block: MacroBlock::default(),
        genesis_header_hash: [0; 32],
        prover_keys_path: PathBuf::from(ZKP_TEST_KEYS_PATH),
    };
    let serialized = postcard::to_allocvec(&proof_input).unwrap();
    let deserialized: ProofInput = postcard::from_bytes(&serialized).unwrap();
    assert_eq!(deserialized, proof_input);
}
