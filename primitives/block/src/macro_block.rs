use std::{fmt, io};

use ark_ec::Group;
use nimiq_bls::{G2Projective, PublicKey as BlsPublicKey};
use nimiq_collections::bitset::BitSet;
use nimiq_hash::{Blake2bHash, Blake2sHash, Hash, HashOutput, Hasher, SerializeContent};
use nimiq_keys::{Address, PublicKey as SchnorrPublicKey};
use nimiq_primitives::{
    policy::Policy,
    slots::{Validators, ValidatorsBuilder},
};
use nimiq_transaction::reward::RewardTransaction;
use nimiq_vrf::VrfSeed;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    signed::{Message, PREFIX_TENDERMINT_PROPOSAL},
    tendermint::TendermintProof,
    BlockError,
};

/// The struct representing a Macro block (can be either checkpoint or election).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MacroBlock {
    /// The header, contains some basic information and commitments to the body and the state.
    pub header: MacroHeader,
    /// The body of the block.
    pub body: Option<MacroBody>,
    /// The justification, contains all the information needed to verify that the header was signed
    /// by the correct producers.
    pub justification: Option<TendermintProof>,
}

impl MacroBlock {
    /// Returns the Blake2b hash of the block header.
    pub fn hash(&self) -> Blake2bHash {
        self.header.hash()
    }

    /// Returns the Blake2s hash of the block header.
    pub fn hash_blake2s(&self) -> Blake2sHash {
        self.header.hash()
    }

    /// Computes the next interlink from self.header.interlink
    pub fn get_next_interlink(&self) -> Result<Vec<Blake2bHash>, BlockError> {
        if !self.is_election_block() {
            return Err(BlockError::InvalidBlockType);
        }
        let mut interlink = self
            .header
            .interlink
            .clone()
            .expect("Election blocks have interlinks");
        let number_hashes_to_update = if self.block_number() == 0 {
            // 0.trailing_zeros() would be 32, thus we need an exception for it
            0
        } else {
            (self.block_number() / Policy::blocks_per_epoch()).trailing_zeros() as usize
        };
        if number_hashes_to_update > interlink.len() {
            interlink.push(self.hash());
        }
        assert!(
            interlink.len() >= number_hashes_to_update,
            "{} {}",
            interlink.len(),
            number_hashes_to_update,
        );
        #[allow(clippy::needless_range_loop)]
        for i in 0..number_hashes_to_update {
            interlink[i] = self.hash();
        }
        Ok(interlink)
    }

    /// Returns whether or not this macro block is an election block.
    pub fn is_election_block(&self) -> bool {
        Policy::is_election_block_at(self.header.block_number)
    }

    /// Returns a copy of the validator slots. Only returns Some if it is an election block.
    pub fn get_validators(&self) -> Option<Validators> {
        self.body.as_ref()?.validators.clone()
    }

    /// Returns the block number of this macro block.
    pub fn block_number(&self) -> u32 {
        self.header.block_number
    }

    /// Returns the block number of this macro block.
    pub fn timestamp(&self) -> u64 {
        self.header.timestamp
    }

    /// Return the round of this macro block.
    pub fn round(&self) -> u32 {
        self.header.round
    }

    /// Returns the epoch number of this macro block.
    pub fn epoch_number(&self) -> u32 {
        Policy::epoch_at(self.header.block_number)
    }

    /// Verifies that the block is valid for the given validators.
    pub(crate) fn verify_validators(&self, validators: &Validators) -> Result<(), BlockError> {
        // Verify the Tendermint proof.
        if !TendermintProof::verify(self, validators) {
            warn!(
                %self,
                reason = "Macro block with bad justification",
                "Rejecting block"
            );
            return Err(BlockError::InvalidJustification);
        }

        Ok(())
    }

    /// Creates a default block that has body and justification.
    pub fn non_empty_default() -> Self {
        let mut validators = ValidatorsBuilder::new();
        for _ in 0..Policy::SLOTS {
            validators.push(
                Address::default(),
                BlsPublicKey::new(G2Projective::generator()).compress(),
                SchnorrPublicKey::default(),
            );
        }

        let validators = Some(validators.build());
        let body = MacroBody {
            validators,
            ..Default::default()
        };
        let body_root = body.hash();
        MacroBlock {
            header: MacroHeader {
                body_root,
                ..Default::default()
            },
            body: Some(body),
            justification: Some(TendermintProof {
                round: 0,
                sig: Default::default(),
            }),
        }
    }
}

impl fmt::Display for MacroBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        fmt::Display::fmt(&self.header, f)
    }
}

/// The struct representing the header of a Macro block (can be either checkpoint or election).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MacroHeader {
    /// The version number of the block. Changing this always results in a hard fork.
    pub version: u16,
    /// The number of the block.
    pub block_number: u32,
    /// The round number this block was proposed in.
    pub round: u32,
    /// The timestamp of the block. It follows the Unix time and has millisecond precision.
    pub timestamp: u64,
    /// The hash of the header of the immediately preceding block (either micro or macro).
    pub parent_hash: Blake2bHash,
    /// The hash of the header of the preceding election macro block.
    pub parent_election_hash: Blake2bHash,
    /// Hashes of the last blocks dividable by 2^x
    pub interlink: Option<Vec<Blake2bHash>>,
    /// The seed of the block. This is the BLS signature of the seed of the immediately preceding
    /// block (either micro or macro) using the validator key of the block proposer.
    pub seed: VrfSeed,
    /// The extra data of the block. It is simply up to 32 raw bytes.
    ///
    /// It encodes the initial supply in the genesis block, as a big-endian `u64`.
    ///
    /// No planned use otherwise.
    pub extra_data: Vec<u8>,
    /// The root of the Merkle tree of the blockchain state. It just acts as a commitment to the
    /// state.
    pub state_root: Blake2bHash,
    /// The root of the Merkle tree of the body. It just acts as a commitment to the body.
    pub body_root: Blake2sHash,
    /// A merkle root over all of the transactions that happened in the current epoch.
    pub history_root: Blake2bHash,
}

impl Message for MacroHeader {
    const PREFIX: u8 = PREFIX_TENDERMINT_PROPOSAL;
}

impl fmt::Display for MacroHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "#{}:MA:{}",
            self.block_number,
            self.hash::<Blake2bHash>().to_short_str(),
        )
    }
}

impl SerializeContent for MacroHeader {
    fn serialize_content<W: std::io::Write, H: HashOutput>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<usize> {
        let mut size = 0;
        let ser_version = postcard::to_allocvec(&self.version)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_version)?;
        size += ser_version.len();
        let ser_block_number = postcard::to_allocvec(&self.block_number)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_block_number)?;
        size += ser_block_number.len();
        let ser_round = postcard::to_allocvec(&self.round)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_round)?;
        size += ser_round.len();

        let ser_timestamp = postcard::to_allocvec(&self.timestamp)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_timestamp)?;
        size += ser_timestamp.len();
        let ser_parent_hash = postcard::to_allocvec(&self.parent_hash)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_parent_hash)?;
        size += ser_parent_hash.len();
        let ser_parent_election_hash = postcard::to_allocvec(&self.parent_election_hash)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_parent_election_hash)?;
        size += ser_parent_election_hash.len();

        let interlink_hash = H::Builder::default()
            .chain(
                &postcard::to_allocvec(&self.interlink)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?,
            )
            .finish();
        let ser_interlink_hash = postcard::to_allocvec(&interlink_hash)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_interlink_hash)?;
        size += ser_interlink_hash.len();

        let ser_seed = postcard::to_allocvec(&self.seed)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_seed)?;
        size += ser_seed.len();
        let ser_extra_data = postcard::to_allocvec(&self.extra_data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_extra_data)?;
        size += ser_extra_data.len();
        let ser_state_root = postcard::to_allocvec(&self.state_root)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_state_root)?;
        size += ser_state_root.len();
        let ser_body_root = postcard::to_allocvec(&self.body_root)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_body_root)?;
        size += ser_body_root.len();
        let ser_history_root = postcard::to_allocvec(&self.history_root)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_history_root)?;
        size += ser_history_root.len();

        Ok(size)
    }
}

/// The struct representing the body of a Macro block (can be either checkpoint or election).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MacroBody {
    /// Contains all the information regarding the next validator set, i.e. their validator
    /// public key, their reward address and their assigned validator slots.
    /// Is only Some when the macro block is an election block.
    pub validators: Option<Validators>,
    /// A bitset representing which validator slots had their reward slashed at the time when this
    /// block was produced. It is used later on for reward distribution.
    pub lost_reward_set: BitSet,
    /// A bitset representing which validator slots were prohibited from producing micro blocks or
    /// proposing macro blocks at the time when this block was produced. It is used later on for
    /// reward distribution.
    pub disabled_set: BitSet,
    /// The reward related transactions of this block.
    pub transactions: Vec<RewardTransaction>,
}

impl MacroBody {
    pub(crate) fn verify(&self, is_election: bool) -> Result<(), BlockError> {
        if is_election != self.validators.is_some() {
            return Err(BlockError::InvalidValidators);
        }

        Ok(())
    }
}

impl SerializeContent for MacroBody {
    fn serialize_content<W: std::io::Write, H: HashOutput>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<usize> {
        let mut size = 0;
        // PITODO: do we need to hash something if None?
        if let Some(ref validators) = self.validators {
            let pk_tree_root = validators.hash::<H>();
            let ser_pk_tree_root = postcard::to_allocvec(&pk_tree_root)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            writer.write_all(&ser_pk_tree_root)?;
            size += ser_pk_tree_root.len();
        } else {
            let ser_zero_byte =
                postcard::to_allocvec(&0u8).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            writer.write_all(&ser_zero_byte)?;
            size += ser_zero_byte.len();
        }
        let ser_lost_reward_set = postcard::to_allocvec(&self.lost_reward_set)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_lost_reward_set)?;
        size += ser_lost_reward_set.len();
        let ser_disabled_set = postcard::to_allocvec(&self.disabled_set)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_disabled_set)?;
        size += ser_disabled_set.len();

        let ser_transactions = postcard::to_allocvec(&self.transactions)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let transactions_hash = ser_transactions.hash::<H>();
        let ser_transactions_hash = postcard::to_allocvec(&transactions_hash)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        writer.write_all(&ser_transactions_hash)?;
        size += ser_transactions_hash.len();

        Ok(size)
    }
}

#[derive(Error, Debug)]
pub enum IntoSlotsError {
    #[error("Body missing in macro block")]
    MissingBody,
    #[error("Not an election macro block")]
    NoElection,
}
