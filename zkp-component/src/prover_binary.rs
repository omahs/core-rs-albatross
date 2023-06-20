use std::io::{self, BufReader, BufWriter, Error, ErrorKind};

use crate::proof_gen_utils::generate_new_proof;
use crate::types::{ProofInput, PROOF_GENERATION_OUTPUT_DELIMITER};
use ark_serialize::{Read, Write};

use crate::types::ZKProofGenerationError;

pub async fn prover_main() -> Result<(), Error> {
    // Read proof input from stdin.
    let mut stdin_buf = vec![];
    let mut stdin = BufReader::new(io::stdin());
    stdin.read_to_end(&mut stdin_buf)?;

    let proof_input: Result<ProofInput, _> = postcard::from_bytes(&stdin_buf);

    log::info!(
        "Starting proof generation for block {:?}",
        proof_input.as_ref().map(|input| &input.block)
    );

    // Then generate proof.
    let result = match proof_input {
        Ok(proof_input) => generate_new_proof(
            proof_input.block,
            proof_input.latest_pks,
            proof_input.latest_header_hash.into(),
            proof_input.previous_proof,
            proof_input.genesis_state,
            &proof_input.prover_keys_path,
        ),
        Err(e) => Err(ZKProofGenerationError::from(e)),
    };
    log::info!("Finished proof generation with result {:?}", result);

    // Then print delimiter followed by the serialized result.
    let mut stdout = BufWriter::new(io::stdout());
    stdout.write_all(&PROOF_GENERATION_OUTPUT_DELIMITER)?;
    stdout
        .write_all(&postcard::to_allocvec(&result).map_err(|e| Error::new(ErrorKind::Other, e))?)?;

    stdout.flush()?;

    Ok(())
}
