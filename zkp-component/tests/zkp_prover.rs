use std::{env, path::PathBuf, str::FromStr};

use nimiq_test_log::test;
use nimiq_zkp_component::{
    proof_gen_utils::launch_generate_new_proof,
    types::{ProofInput, ZKProofGenerationError},
};
use tokio::sync::oneshot;

pub fn zkp_test_exe() -> PathBuf {
    let prover_path = PathBuf::from_str(
        &env::var("CARGO_BIN_EXE_nimiq-test-prover").expect("Run this with all features!"),
    )
    .unwrap();

    assert!(
        prover_path.exists(),
        "Run `cargo build --bin=nimiq-test-prover --all-features` to build the test prover binary at {prover_path:?}"
    );
    prover_path
}

#[test]
#[cfg_attr(not(feature = "test-prover"), ignore)]
fn can_locate_prover_binary() {
    zkp_test_exe();
}

#[test(tokio::test)]
#[cfg_attr(not(feature = "test-prover"), ignore)]
async fn can_launch_process_and_parse_output() {
    let (_send, recv) = oneshot::channel();
    let proof_input: ProofInput = Default::default();

    let result = launch_generate_new_proof(recv, proof_input, Some(zkp_test_exe())).await;

    assert_eq!(
        result,
        Err(ZKProofGenerationError::NanoZKP("invalid block".to_string()))
    );
}

#[test(tokio::test)]
#[cfg_attr(not(feature = "test-prover"), ignore)]
async fn can_launch_process_and_kill() {
    let (send, recv) = oneshot::channel();
    let proof_input: ProofInput = Default::default();

    let result = tokio::spawn(launch_generate_new_proof(
        recv,
        proof_input,
        Some(zkp_test_exe()),
    ));
    send.send(()).unwrap();

    assert_eq!(
        result.await.unwrap(),
        Err(ZKProofGenerationError::ChannelError)
    );
}
