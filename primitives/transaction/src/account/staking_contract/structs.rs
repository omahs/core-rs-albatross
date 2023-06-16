use log::error;

use nimiq_bls::{CompressedPublicKey as BlsPublicKey, CompressedSignature as BlsSignature};
use nimiq_hash::Blake2bHash;
use nimiq_keys::{Address, PublicKey as SchnorrPublicKey};
use nimiq_primitives::coin::Coin;
use nimiq_primitives::policy::Policy;
use serde::{Deserialize, Serialize};

use crate::SignatureProof;
use crate::{Transaction, TransactionError};

/// We need to distinguish two types of transactions:
/// 1. Incoming transactions, which include:
///     - Validator
///         * Create
///         * Update
///         * Unpark
///         * Deactivate
///         * Reactivate
///         * Retire
///     - Staker
///         * Create
///         * Update
///         * AddStake
///     The type of transaction, parameters and proof are given in the data field of the transaction.
/// 2. Outgoing transactions, which include:
///     - Validator
///         * Delete
///     - Staker
///         * RemoveStake
///     The type of transaction, parameters and proof are given in the proof field of the transaction.
///
/// It is important to note that all `signature` fields contain the signature
/// over the complete transaction with the `signature` field set to `Default::default()`.
/// The field is populated only after computing the signature.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum IncomingStakingTransactionData {
    CreateValidator {
        signing_key: SchnorrPublicKey,
        voting_key: BlsPublicKey,
        reward_address: Address,
        signal_data: Option<Blake2bHash>,
        proof_of_knowledge: BlsSignature,
        // This proof is signed with the validator cold key, which will become the validator address.
        #[cfg_attr(feature = "serde-derive", serde(skip))]
        proof: SignatureProof,
    },
    UpdateValidator {
        new_signing_key: Option<SchnorrPublicKey>,
        new_voting_key: Option<BlsPublicKey>,
        new_reward_address: Option<Address>,
        new_signal_data: Option<Option<Blake2bHash>>,
        new_proof_of_knowledge: Option<BlsSignature>,
        // This proof is signed with the validator cold key.
        #[cfg_attr(feature = "serde-derive", serde(skip))]
        proof: SignatureProof,
    },
    UnparkValidator {
        validator_address: Address,
        // This proof is signed with the validator warm key.
        #[cfg_attr(feature = "serde-derive", serde(skip))]
        proof: SignatureProof,
    },
    DeactivateValidator {
        validator_address: Address,
        // This proof is signed with the validator warm key.
        #[cfg_attr(feature = "serde-derive", serde(skip))]
        proof: SignatureProof,
    },
    ReactivateValidator {
        validator_address: Address,
        // This proof is signed with the validator warm key.
        #[cfg_attr(feature = "serde-derive", serde(skip))]
        proof: SignatureProof,
    },
    RetireValidator {
        // This proof is signed with the validator cold key.
        #[cfg_attr(feature = "serde-derive", serde(skip))]
        proof: SignatureProof,
    },
    CreateStaker {
        delegation: Option<Address>,
        #[cfg_attr(feature = "serde-derive", serde(skip))]
        proof: SignatureProof,
    },
    AddStake {
        staker_address: Address,
    },
    UpdateStaker {
        new_delegation: Option<Address>,
        #[cfg_attr(feature = "serde-derive", serde(skip))]
        proof: SignatureProof,
    },
}

impl IncomingStakingTransactionData {
    pub fn is_signaling(&self) -> bool {
        matches!(
            self,
            IncomingStakingTransactionData::UpdateValidator { .. }
                | IncomingStakingTransactionData::UnparkValidator { .. }
                | IncomingStakingTransactionData::DeactivateValidator { .. }
                | IncomingStakingTransactionData::ReactivateValidator { .. }
                | IncomingStakingTransactionData::RetireValidator { .. }
                | IncomingStakingTransactionData::UpdateStaker { .. }
        )
    }

    pub fn parse(transaction: &Transaction) -> Result<Self, TransactionError> {
        full_parse(&transaction.data[..])
    }

    pub fn verify(&self, transaction: &Transaction) -> Result<(), TransactionError> {
        match self {
            IncomingStakingTransactionData::CreateValidator {
                voting_key,
                proof_of_knowledge,
                proof,
                ..
            } => {
                // Validators must be created with exactly the validator deposit amount.
                if transaction.value != Coin::from_u64_unchecked(Policy::VALIDATOR_DEPOSIT) {
                    error!("Validator stake value different from VALIDATOR_DEPOSIT. The offending transaction is the following:\n{:?}", transaction);
                    return Err(TransactionError::InvalidValue);
                }

                // Check proof of knowledge.
                verify_proof_of_knowledge(voting_key, proof_of_knowledge)?;

                // Check that the signature is correct.
                verify_transaction_signature(transaction, proof, true)?
            }
            IncomingStakingTransactionData::UpdateValidator {
                new_signing_key,
                new_voting_key,
                new_reward_address,
                new_signal_data,
                new_proof_of_knowledge,
                proof,
            } => {
                // Do not allow updates without any effect.
                if new_signing_key.is_none()
                    && new_voting_key.is_none()
                    && new_reward_address.is_none()
                    && new_signal_data.is_none()
                {
                    error!("Signaling update transactions must actually update something. The offending transaction is the following:\n{:?}", transaction);
                    return Err(TransactionError::InvalidData);
                }

                // Check proof of knowledge, if necessary.
                if let (Some(new_voting_key), Some(new_proof_of_knowledge)) =
                    (new_voting_key, new_proof_of_knowledge)
                {
                    verify_proof_of_knowledge(new_voting_key, new_proof_of_knowledge)?;
                }

                // Check that the signature is correct.
                verify_transaction_signature(transaction, proof, true)?
            }
            IncomingStakingTransactionData::UnparkValidator { proof, .. } => {
                // Check that the signature is correct.
                verify_transaction_signature(transaction, proof, true)?
            }
            IncomingStakingTransactionData::DeactivateValidator { proof, .. } => {
                // Check that the signature is correct.
                verify_transaction_signature(transaction, proof, true)?
            }
            IncomingStakingTransactionData::ReactivateValidator { proof, .. } => {
                // Check that the signature is correct.
                verify_transaction_signature(transaction, proof, true)?
            }
            IncomingStakingTransactionData::RetireValidator { proof, .. } => {
                // Check that the signature is correct.
                verify_transaction_signature(transaction, proof, true)?
            }
            IncomingStakingTransactionData::CreateStaker { proof, .. } => {
                // Check that stake is bigger than zero.
                if transaction.value.is_zero() {
                    warn!("Can't create a staker with zero balance. The offending transaction is the following:\n{:?}", transaction);
                    return Err(TransactionError::ZeroValue);
                }

                // Check that the signature is correct.
                verify_transaction_signature(transaction, proof, true)?
            }
            IncomingStakingTransactionData::AddStake { .. } => {
                // No checks needed.
            }
            IncomingStakingTransactionData::UpdateStaker { proof, .. } => {
                // Check that the signature is correct.
                verify_transaction_signature(transaction, proof, true)?
            }
        }

        Ok(())
    }

    pub fn set_signature(&mut self, signature_proof: SignatureProof) {
        match self {
            IncomingStakingTransactionData::CreateValidator { proof, .. } => {
                *proof = signature_proof;
            }
            IncomingStakingTransactionData::UpdateValidator { proof, .. } => {
                *proof = signature_proof;
            }
            IncomingStakingTransactionData::UnparkValidator { proof, .. } => {
                *proof = signature_proof;
            }
            IncomingStakingTransactionData::DeactivateValidator { proof, .. } => {
                *proof = signature_proof;
            }
            IncomingStakingTransactionData::ReactivateValidator { proof, .. } => {
                *proof = signature_proof;
            }
            IncomingStakingTransactionData::RetireValidator { proof, .. } => {
                *proof = signature_proof;
            }
            IncomingStakingTransactionData::CreateStaker { proof, .. } => {
                *proof = signature_proof;
            }
            IncomingStakingTransactionData::UpdateStaker { proof, .. } => {
                *proof = signature_proof;
            }
            _ => {}
        }
    }

    pub fn set_signature_on_data(
        data: &[u8],
        signature_proof: SignatureProof,
    ) -> Result<Vec<u8>, postcard::Error> {
        let mut data: IncomingStakingTransactionData = postcard::from_bytes(data)?;
        data.set_signature(signature_proof);
        postcard::to_allocvec(&data)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum OutgoingStakingTransactionProof {
    DeleteValidator {
        #[cfg_attr(feature = "serde-derive", serde(skip))]
        // This proof is signed with the validator cold key.
        proof: SignatureProof,
    },
    RemoveStake {
        #[cfg_attr(feature = "serde-derive", serde(skip))]
        proof: SignatureProof,
    },
}

impl OutgoingStakingTransactionProof {
    pub fn parse(transaction: &Transaction) -> Result<Self, TransactionError> {
        full_parse(&transaction.proof[..])
    }

    pub fn verify(&self, transaction: &Transaction) -> Result<(), TransactionError> {
        match self {
            OutgoingStakingTransactionProof::DeleteValidator { proof } => {
                // Check that the signature is correct.
                verify_transaction_signature(transaction, proof, false)?
            }
            OutgoingStakingTransactionProof::RemoveStake { proof } => {
                // Check that the signature is correct.
                verify_transaction_signature(transaction, proof, false)?
            }
        }

        Ok(())
    }
}

pub fn full_parse<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, TransactionError> {
    let (data, left_over) = postcard::take_from_bytes(data)?;

    // Ensure that transaction data has been fully read.
    if !left_over.is_empty() {
        return Err(TransactionError::InvalidData);
    }

    Ok(data)
}

pub fn verify_transaction_signature(
    transaction: &Transaction,
    sig_proof: &SignatureProof,
    incoming: bool,
) -> Result<(), TransactionError> {
    // If we are verifying the signature on an incoming transaction, then we need to reset the
    // signature field first.
    let tx = if incoming {
        let mut tx_without_sig = transaction.clone();

        tx_without_sig.data = IncomingStakingTransactionData::set_signature_on_data(
            &tx_without_sig.data,
            SignatureProof::default(),
        )?;

        tx_without_sig.serialize_content()
    } else {
        transaction.serialize_content()
    };

    if !sig_proof.verify(&tx) {
        error!(
            "Invalid proof. The offending transaction is the following:\n{:?}",
            transaction
        );
        return Err(TransactionError::InvalidProof);
    }

    Ok(())
}

/// Important: Currently, the proof of knowledge of the secret key is a signature of the public key.
/// If an attacker A ever tricks a validator B into signing a message with content `pk_A - pk_B`,
/// where `pk_X` is X's BLS public key, A will be able to sign aggregate messages that are valid for
/// public keys `pk_B + (pk_A - pk_B) = pk_B`.
/// Alternatives would be to replace the proof of knowledge by a zero-knowledge proof.
pub fn verify_proof_of_knowledge(
    voting_key: &BlsPublicKey,
    proof_of_knowledge: &BlsSignature,
) -> Result<(), TransactionError> {
    if !voting_key
        .uncompress()
        .map_err(|_| TransactionError::InvalidData)?
        .verify(
            voting_key,
            &proof_of_knowledge
                .uncompress()
                .map_err(|_| TransactionError::InvalidData)?,
        )
    {
        error!("Verification of the proof of knowledge for a BLS key failed! For the following BLS public key:\n{:?}",
            voting_key);
        return Err(TransactionError::InvalidData);
    }

    Ok(())
}
