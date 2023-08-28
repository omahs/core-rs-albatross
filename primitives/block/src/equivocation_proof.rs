use std::{cmp::Ordering, hash::Hasher, mem};

use nimiq_bls::{AggregatePublicKey, AggregateSignature};
use nimiq_collections::BitSet;
use nimiq_hash::{Blake2bHash, Blake2sHash, Hash, HashOutput};
use nimiq_hash_derive::SerializeContent;
use nimiq_keys::{Address, PublicKey as SchnorrPublicKey, Signature as SchnorrSignature};
use nimiq_primitives::{policy::Policy, slots_allocation::Validators};
use nimiq_serde::{Deserialize, Serialize};
use nimiq_vrf::VrfSeed;
use thiserror::Error;

use crate::{MacroHeader, MicroHeader, TendermintIdentifier, TendermintVote};

/// An equivocation proof proves that a validator misbehaved.
///
/// This can come in several forms, but e.g. producing two blocks in a single slot or voting twice
/// in the same round.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, SerializeContent)]
pub enum EquivocationProof {
    Fork(ForkProof),
    DoubleProposal(DoubleProposalProof),
    DoubleVote(DoubleVoteProof),
}

const fn max3(a: usize, b: usize, c: usize) -> usize {
    if a > b && a > c {
        a
    } else if b > c {
        b
    } else {
        c
    }
}

impl EquivocationProof {
    /// The size of a single equivocation proof. This is the maximum possible size.
    pub const MAX_SIZE: usize = 1 + max3(
        ForkProof::MAX_SIZE,
        DoubleProposalProof::MAX_SIZE,
        DoubleVoteProof::MAX_SIZE,
    );

    /// Returns the block number of an equivocation proof. This assumes that the equivocation proof
    /// is valid.
    pub fn block_number(&self) -> u32 {
        use self::EquivocationProof::*;
        match self {
            Fork(proof) => proof.block_number(),
            DoubleProposal(proof) => proof.block_number(),
            DoubleVote(proof) => proof.block_number(),
        }
    }

    /// Check if an equivocation proof is valid at a given block height. Equivocation proofs are
    /// valid only until the end of the reporting window.
    pub fn is_valid_at(&self, block_number: u32) -> bool {
        block_number <= Policy::last_block_of_reporting_window(self.block_number())
            && Policy::batch_at(block_number) >= Policy::batch_at(self.block_number())
    }

    /// Returns the key by which equivocation proofs are supposed to be sorted.
    pub fn sort_key(&self) -> Blake2bHash {
        self.hash()
    }
}

impl From<ForkProof> for EquivocationProof {
    fn from(proof: ForkProof) -> EquivocationProof {
        EquivocationProof::Fork(proof)
    }
}

impl From<DoubleProposalProof> for EquivocationProof {
    fn from(proof: DoubleProposalProof) -> EquivocationProof {
        EquivocationProof::DoubleProposal(proof)
    }
}

impl From<DoubleVoteProof> for EquivocationProof {
    fn from(proof: DoubleVoteProof) -> EquivocationProof {
        EquivocationProof::DoubleVote(proof)
    }
}

/// Struct representing a fork proof. A fork proof proves that a given validator created or
/// continued a fork. For this it is enough to provide two different headers, with the same block
/// number, signed by the same validator.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, SerializeContent)]
pub struct ForkProof {
    /// Header number 1.
    header1: MicroHeader,
    /// Header number 2.
    header2: MicroHeader,
    /// Justification for header number 1.
    justification1: SchnorrSignature,
    /// Justification for header number 2.
    justification2: SchnorrSignature,
    /// VRF seed of the previous block. Used to determine the slot.
    prev_vrf_seed: VrfSeed,
}

impl ForkProof {
    /// The size of a single fork proof. This is the maximum possible size, since the Micro header
    /// has a variable size (because of the extra data field) and here we assume that the header
    /// has the maximum size.
    pub const MAX_SIZE: usize =
        2 * MicroHeader::MAX_SIZE + 2 * SchnorrSignature::SIZE + VrfSeed::SIZE;

    pub fn new(
        mut header1: MicroHeader,
        mut justification1: SchnorrSignature,
        mut header2: MicroHeader,
        mut justification2: SchnorrSignature,
        prev_vrf_seed: VrfSeed,
    ) -> ForkProof {
        let hash1: Blake2bHash = header1.hash();
        let hash2: Blake2bHash = header2.hash();
        if hash1 > hash2 {
            mem::swap(&mut header1, &mut header2);
            mem::swap(&mut justification1, &mut justification2);
        }
        ForkProof {
            header1,
            header2,
            justification1,
            justification2,
            prev_vrf_seed,
        }
    }

    /// Hash of header number 1.
    pub fn header1_hash(&self) -> Blake2bHash {
        self.header1.hash()
    }
    /// Hash of header number 2.
    pub fn header2_hash(&self) -> Blake2bHash {
        self.header2.hash()
    }
    /// Block number.
    pub fn block_number(&self) -> u32 {
        self.header1.block_number
    }
    /// VRF seed of the previous block. Used to determine the slot.
    pub fn prev_vrf_seed(&self) -> &VrfSeed {
        &self.prev_vrf_seed
    }

    /// Verify the validity of a fork proof.
    pub fn verify(&self, signing_key: &SchnorrPublicKey) -> Result<(), EquivocationProofError> {
        let hash1: Blake2bHash = self.header1.hash();
        let hash2: Blake2bHash = self.header2.hash();

        // Check that the headers are not equal and in the right order:
        match hash1.cmp(&hash2) {
            Ordering::Less => {}
            Ordering::Equal => return Err(EquivocationProofError::SameHeader),
            Ordering::Greater => return Err(EquivocationProofError::WrongOrder),
        }

        // Check that the headers have equal block numbers and seeds.
        if self.header1.block_number != self.header2.block_number
            || self.header1.seed.entropy() != self.header2.seed.entropy()
        {
            return Err(EquivocationProofError::SlotMismatch);
        }

        if let Err(error) = self.header1.seed.verify(&self.prev_vrf_seed, signing_key) {
            error!(?error, "ForkProof: VrfSeed failed to verify");
            return Err(EquivocationProofError::InvalidJustification);
        }

        // Check that the justifications are valid.
        let hash1 = self.header1.hash::<Blake2bHash>();
        let hash2 = self.header2.hash::<Blake2bHash>();
        if !signing_key.verify(&self.justification1, hash1.as_slice())
            || !signing_key.verify(&self.justification2, hash2.as_slice())
        {
            return Err(EquivocationProofError::InvalidJustification);
        }

        Ok(())
    }
}

impl std::hash::Hash for ForkProof {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.header1.hash::<Blake2bHash>().as_bytes());
        state.write(self.header2.hash::<Blake2bHash>().as_bytes());
    }
}

/// Possible equivocation proof validation errors.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum EquivocationProofError {
    #[error("Slot mismatch")]
    SlotMismatch,
    #[error("Invalid justification")]
    InvalidJustification,
    #[error("Invalid validator address")]
    InvalidValidatorAddress,
    #[error("Same header")]
    SameHeader,
    #[error("Wrong order")]
    WrongOrder,
}

/// Struct representing a double proposal proof. A double proposal proof proves that a given
/// validator created two macro block proposals at the same height, in the same round.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, SerializeContent)]
pub struct DoubleProposalProof {
    /// Header number 1.
    header1: MacroHeader,
    /// Header number 2.
    header2: MacroHeader,
    /// Justification for header number 1.
    justification1: SchnorrSignature,
    /// Justification for header number 2.
    justification2: SchnorrSignature,
    /// VRF seed of the previous block for header 1. Used to determine the slot.
    prev_vrf_seed1: VrfSeed,
    /// VRF seed of the previous block for header 2. Used to determine the slot.
    prev_vrf_seed2: VrfSeed,
}

impl DoubleProposalProof {
    /// The maximum size of a double proposal proof.
    pub const MAX_SIZE: usize =
        2 * MacroHeader::MAX_SIZE + 2 * SchnorrSignature::SIZE + 2 * VrfSeed::SIZE;

    pub fn new(
        mut header1: MacroHeader,
        mut justification1: SchnorrSignature,
        mut prev_vrf_seed1: VrfSeed,
        mut header2: MacroHeader,
        mut justification2: SchnorrSignature,
        mut prev_vrf_seed2: VrfSeed,
    ) -> DoubleProposalProof {
        let hash1: Blake2bHash = header1.hash();
        let hash2: Blake2bHash = header2.hash();
        if hash1 > hash2 {
            mem::swap(&mut header1, &mut header2);
            mem::swap(&mut justification1, &mut justification2);
            mem::swap(&mut prev_vrf_seed1, &mut prev_vrf_seed2);
        }
        DoubleProposalProof {
            header1,
            header2,
            justification1,
            justification2,
            prev_vrf_seed1,
            prev_vrf_seed2,
        }
    }

    /// Block number.
    pub fn block_number(&self) -> u32 {
        self.header1.block_number
    }
    /// Round of the proposals.
    pub fn round(&self) -> u32 {
        self.header1.round
    }
    /// Hash of header number 1.
    pub fn header1_hash(&self) -> Blake2bHash {
        self.header1.hash()
    }
    /// Hash of header number 2.
    pub fn header2_hash(&self) -> Blake2bHash {
        self.header2.hash()
    }
    /// VRF seed of the previous block for header 1. Used to determine the slot.
    pub fn prev_vrf_seed1(&self) -> &VrfSeed {
        &self.prev_vrf_seed1
    }
    /// VRF seed of the previous block for header 2. Used to determine the slot.
    pub fn prev_vrf_seed2(&self) -> &VrfSeed {
        &self.prev_vrf_seed2
    }

    /// Verify the validity of a double proposal proof.
    pub fn verify(&self, signing_key: &SchnorrPublicKey) -> Result<(), EquivocationProofError> {
        let hash1: Blake2bHash = self.header1.hash();
        let hash2: Blake2bHash = self.header2.hash();

        // Check that the headers are not equal and in the right order:
        match hash1.cmp(&hash2) {
            Ordering::Less => {}
            Ordering::Equal => return Err(EquivocationProofError::SameHeader),
            Ordering::Greater => return Err(EquivocationProofError::WrongOrder),
        }

        if self.header1.block_number != self.header2.block_number
            || self.header1.round != self.header2.round
            || self.header1.seed.entropy() != self.header2.seed.entropy()
        {
            return Err(EquivocationProofError::SlotMismatch);
        }

        if let Err(error) = self.header1.seed.verify(&self.prev_vrf_seed1, signing_key) {
            error!(?error, "DoubleProposalProof: VrfSeed 1 failed to verify");
            return Err(EquivocationProofError::InvalidJustification);
        }

        if let Err(error) = self.header2.seed.verify(&self.prev_vrf_seed2, signing_key) {
            error!(?error, "DoubleProposalProof: VrfSeed 2 failed to verify");
            return Err(EquivocationProofError::InvalidJustification);
        }

        // Check that the justifications are valid.
        let hash1 = self.header1.hash::<Blake2bHash>();
        let hash2 = self.header2.hash::<Blake2bHash>();
        if !signing_key.verify(&self.justification1, hash1.as_slice())
            || !signing_key.verify(&self.justification2, hash2.as_slice())
        {
            return Err(EquivocationProofError::InvalidJustification);
        }

        Ok(())
    }
}

impl std::hash::Hash for DoubleProposalProof {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(Hash::hash::<Blake2bHash>(self).as_bytes());
    }
}

/// Struct representing a double vote proof. A double vote proof proves that a given
/// validator voted twice at same height, in the same round.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, SerializeContent)]
pub struct DoubleVoteProof {
    /// Round and block height information.
    tendermint_id: TendermintIdentifier,
    /// Address of the offending validator.
    validator_address: Address,
    /// Hash of proposal number 1.
    proposal_hash1: Option<Blake2sHash>,
    /// Hash of proposal number 2.
    proposal_hash2: Option<Blake2sHash>,
    /// Aggregate signature for proposal 1.
    signature1: AggregateSignature,
    /// Aggregate signature for proposal 2.
    signature2: AggregateSignature,
    /// Signers for proposal 1.
    signers1: BitSet,
    /// Signers for proposal 2.
    signers2: BitSet,
}

impl DoubleVoteProof {
    /// The maximum size of a double proposal proof.
    pub const MAX_SIZE: usize = 2 * MacroHeader::MAX_SIZE
        + 2 * nimiq_serde::option_max_size(Blake2sHash::SIZE)
        + 2 * AggregateSignature::SIZE
        + 2 * BitSet::max_size(Policy::SLOTS as usize);

    pub fn new(
        tendermint_id: TendermintIdentifier,
        validator_address: Address,
        mut proposal_hash1: Option<Blake2sHash>,
        mut proposal_hash2: Option<Blake2sHash>,
        mut signature1: AggregateSignature,
        mut signature2: AggregateSignature,
        mut signers1: BitSet,
        mut signers2: BitSet,
    ) -> DoubleVoteProof {
        if proposal_hash1 > proposal_hash2 {
            mem::swap(&mut proposal_hash1, &mut proposal_hash2);
            mem::swap(&mut signature1, &mut signature2);
            mem::swap(&mut signers1, &mut signers2);
        }
        DoubleVoteProof {
            tendermint_id,
            validator_address,
            proposal_hash1,
            proposal_hash2,
            signature1,
            signature2,
            signers1,
            signers2,
        }
    }

    /// Block number.
    pub fn block_number(&self) -> u32 {
        self.tendermint_id.block_number
    }
    /// Address of the offending validator.
    pub fn validator_address(&self) -> &Address {
        &self.validator_address
    }

    /// Verify the validity of a double proposal proof.
    pub fn verify(&self, validators: &Validators) -> Result<(), EquivocationProofError> {
        // Check that the proposals are not equal and in the right order:
        match self.proposal_hash1.cmp(&self.proposal_hash2) {
            Ordering::Less => {}
            Ordering::Equal => return Err(EquivocationProofError::SameHeader),
            Ordering::Greater => return Err(EquivocationProofError::WrongOrder),
        }

        let validator = match validators.get_validator_by_address(&self.validator_address) {
            None => return Err(EquivocationProofError::InvalidValidatorAddress),
            Some(v) => v,
        };

        // Check that at least one of the validator's slots is actually contained in both signer sets.
        if !validator
            .slots
            .clone()
            .any(|s| self.signers1.contains(s as usize) && self.signers2.contains(s as usize))
        {
            return Err(EquivocationProofError::SlotMismatch);
        }

        let verify =
            |proposal_hash, signers: &BitSet, signature| -> Result<(), EquivocationProofError> {
                // Calculate the message that was actually signed by the validators.
                let message = TendermintVote {
                    proposal_hash,
                    id: self.tendermint_id.clone(),
                };
                // Verify the signatures.
                let mut agg_pk = AggregatePublicKey::new();
                for (i, pk) in validators.voting_keys().iter().enumerate() {
                    if signers.contains(i) {
                        agg_pk.aggregate(pk);
                    }
                }
                if !agg_pk.verify(&message, signature) {
                    return Err(EquivocationProofError::InvalidJustification);
                }
                Ok(())
            };

        verify(
            self.proposal_hash1.clone(),
            &self.signers1,
            &self.signature1,
        )?;
        verify(
            self.proposal_hash2.clone(),
            &self.signers2,
            &self.signature2,
        )?;
        Ok(())
    }
}

impl std::hash::Hash for DoubleVoteProof {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(Hash::hash::<Blake2bHash>(self).as_bytes());
    }
}
