use std::io;
use std::sync::Arc;

use ark_groth16::Proof;
use ark_mnt6_753::{G2Projective as G2MNT6, MNT6_753};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};

use nimiq_block::MacroBlock;
use nimiq_blockchain_interface::AbstractBlockchain;
use nimiq_blockchain_proxy::BlockchainProxy;
use nimiq_database_value::{AsDatabaseBytes, FromDatabaseValue};
use nimiq_hash::Blake2bHash;
use nimiq_network_interface::network::Network;
use nimiq_network_interface::request::{Handle, RequestError};
use nimiq_network_interface::{
    network::Topic,
    request::{RequestCommon, RequestMarker},
};
use nimiq_zkp_primitives::MacroBlock as ZKPMacroBlock;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::path::PathBuf;

use nimiq_zkp_primitives::NanoZKPError;
use thiserror::Error;

use crate::ZKPComponent;

pub const PROOF_GENERATION_OUTPUT_DELIMITER: [u8; 2] = [242, 208];

/// The ZKP event returned by the stream.
#[derive(Debug)]
pub struct ZKPEvent<N: Network> {
    pub source: ProofSource<N>,
    pub proof: ZKProof,
    pub block: MacroBlock,
}

impl<N: Network> ZKPEvent<N> {
    pub fn new(source: ProofSource<N>, proof: ZKProof, block: MacroBlock) -> Self {
        ZKPEvent {
            source,
            proof,
            block,
        }
    }
}

impl<N: Network> Clone for ZKPEvent<N> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            proof: self.proof.clone(),
            block: self.block.clone(),
        }
    }
}

/// The ZKP event returned for individual requests by the ZKP requests component.
#[derive(Debug)]
pub enum ZKPRequestEvent {
    /// A valid proof that has been pushed to the ZKP state.
    Proof { proof: ZKProof, block: MacroBlock },
    /// The peer does not have a more recent proof.
    OutdatedProof { block_height: u32 },
}

/// The ZK Proof state containing the pks block info and the proof.
/// The genesis block has no zk proof.
#[derive(Clone, Debug, PartialEq)]
pub struct ZKPState {
    pub latest_pks: Vec<G2MNT6>,
    pub latest_header_hash: Blake2bHash,
    pub latest_block_number: u32,
    pub latest_proof: Option<Proof<MNT6_753>>,
}

impl ZKPState {
    pub fn with_genesis(genesis_block: &MacroBlock) -> Result<Self, Error> {
        let latest_pks: Vec<_> = genesis_block
            .get_validators()
            .ok_or(Error::InvalidBlock)?
            .voting_keys()
            .into_iter()
            .map(|pub_key| pub_key.public_key)
            .collect();

        let genesis_block =
            ZKPMacroBlock::try_from(genesis_block).map_err(|_| Error::InvalidBlock)?;

        Ok(ZKPState {
            latest_pks,
            latest_header_hash: genesis_block.header_hash.into(),
            latest_block_number: genesis_block.block_number,
            latest_proof: None,
        })
    }
}

/// Contains the id of the source of the newly pushed proof. This object is sent through the network alongside the zk proof.
#[derive(Copy, Debug)]
pub enum ProofSource<N: Network> {
    PeerGenerated(N::PeerId),
    SelfGenerated,
}

impl<N: Network> Clone for ProofSource<N> {
    fn clone(&self) -> Self {
        match self {
            Self::PeerGenerated(peer_id) => Self::PeerGenerated(*peer_id),
            Self::SelfGenerated => Self::SelfGenerated,
        }
    }
}

impl<N: Network> ProofSource<N> {
    pub fn unwrap_peer_id(&self) -> N::PeerId {
        match self {
            Self::PeerGenerated(peer_id) => *peer_id,
            Self::SelfGenerated => panic!("Called unwrap_peer_id on a self generated proof source"),
        }
    }
}

/// The ZK Proof and the respective block identifier. This object is sent though the network and stored in the zkp db.
#[derive(Clone, Debug, PartialEq)]
pub struct ZKProof {
    pub block_number: u32,
    pub proof: Option<Proof<MNT6_753>>,
}

impl ZKProof {
    pub fn new(block_number: u32, proof: Option<Proof<MNT6_753>>) -> Self {
        Self {
            block_number,
            proof,
        }
    }
}

impl From<ZKPState> for ZKProof {
    fn from(zkp_component_state: ZKPState) -> Self {
        Self {
            block_number: zkp_component_state.latest_block_number,
            proof: zkp_component_state.latest_proof,
        }
    }
}

impl AsDatabaseBytes for ZKProof {
    fn as_database_bytes(&self) -> Cow<[u8]> {
        let v = postcard::to_allocvec(self).unwrap();
        Cow::Owned(v)
    }
}

impl FromDatabaseValue for ZKProof {
    fn copy_from_database(bytes: &[u8]) -> io::Result<Self>
    where
        Self: Sized,
    {
        postcard::from_bytes(&bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

/// The input to the proof generation process.
#[derive(Clone, Debug, PartialEq)]
pub struct ProofInput {
    pub block: MacroBlock,
    pub latest_pks: Vec<G2MNT6>,
    pub latest_header_hash: Blake2bHash,
    pub previous_proof: Option<Proof<MNT6_753>>,
    pub genesis_state: [u8; 95],
    pub prover_keys_path: PathBuf,
}

impl Default for ProofInput {
    fn default() -> Self {
        Self {
            block: Default::default(),
            latest_pks: Default::default(),
            latest_header_hash: Default::default(),
            previous_proof: Default::default(),
            genesis_state: [0; 95],
            prover_keys_path: Default::default(),
        }
    }
}

/// The topic for zkp gossiping.
#[derive(Clone, Debug, Default)]
pub struct ZKProofTopic;

impl Topic for ZKProofTopic {
    type Item = ZKProof;

    const BUFFER_SIZE: usize = 16;
    const NAME: &'static str = "zkproofs";
    const VALIDATE: bool = true;
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Nano Zkp Error: {0}")]
    NanoZKP(#[from] NanoZKPError),

    #[error("Proof's blocks are not valid")]
    InvalidBlock,

    #[error("Outdated proof")]
    OutdatedProof,

    #[error("Invalid proof")]
    InvalidProof,

    #[error("Request Error: {0}")]
    Request(#[from] RequestError),
}

#[derive(Error, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum ZKProofGenerationError {
    #[error("Nano Zkp Error: {0}")]
    NanoZKP(String),

    #[error("Serialization Error: {0}")]
    SerializingError(String),

    #[error("Proof's blocks are not valid")]
    InvalidBlock,

    #[error("Channel error")]
    ChannelError,

    #[error("Process launching error: {0}")]
    ProcessError(String),
}

impl From<postcard::Error> for ZKProofGenerationError {
    fn from(e: postcard::Error) -> Self {
        ZKProofGenerationError::SerializingError(e.to_string())
    }
}

impl From<NanoZKPError> for ZKProofGenerationError {
    fn from(e: NanoZKPError) -> Self {
        ZKProofGenerationError::NanoZKP(e.to_string())
    }
}

impl From<io::Error> for ZKProofGenerationError {
    fn from(e: io::Error) -> Self {
        ZKProofGenerationError::ProcessError(e.to_string())
    }
}

/// The max number of ZKP requests per peer.
pub const MAX_REQUEST_RESPONSE_ZKP: u32 = 1000;

/// The request of a zkp. The request specifies the block height to be used as a filtering mechanism to avoid flooding the network
/// with older proofs.
/// The response should either have a more recent proof (> than block_number) or None.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestZKP {
    pub(crate) block_number: u32,
    pub(crate) request_election_block: bool,
}

impl RequestCommon for RequestZKP {
    type Kind = RequestMarker;
    const TYPE_ID: u16 = 211;
    type Response = RequestZKPResponse;

    const MAX_REQUESTS: u32 = MAX_REQUEST_RESPONSE_ZKP;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(u8)]
pub enum RequestZKPResponse {
    Proof(ZKProof, Option<MacroBlock>),
    Outdated(u32),
}

#[derive(Clone)]
pub(crate) struct ZKPStateEnvironment {
    pub(crate) zkp_state: Arc<RwLock<ZKPState>>,
    pub(crate) blockchain: BlockchainProxy,
}

impl<N: Network> From<&ZKPComponent<N>> for ZKPStateEnvironment {
    fn from(component: &ZKPComponent<N>) -> Self {
        ZKPStateEnvironment {
            zkp_state: Arc::clone(&component.zkp_state),
            blockchain: component.blockchain.clone(),
        }
    }
}

impl<N: Network> Handle<N, RequestZKPResponse, Arc<ZKPStateEnvironment>> for RequestZKP {
    fn handle(&self, _peer_id: N::PeerId, env: &Arc<ZKPStateEnvironment>) -> RequestZKPResponse {
        // First retrieve the ZKP proof and release the lock again.
        let zkp_state = env.zkp_state.read();
        let latest_block_number = zkp_state.latest_block_number;
        if latest_block_number <= self.block_number {
            return RequestZKPResponse::Outdated(latest_block_number);
        }
        let zkp_proof = (*zkp_state).clone().into();
        drop(zkp_state);

        // Then get the corresponding block if necessary.
        let block = if self.request_election_block {
            env.blockchain
                .read()
                .get_block_at(latest_block_number, true)
                .ok()
                .map(|block| block.unwrap_macro())
        } else {
            None
        };
        RequestZKPResponse::Proof(zkp_proof, block)
    }
}

mod serde_derive {

    use std::fmt;

    use ark_serialize::Write;
    use serde::{
        de::{Deserialize, Deserializer, Error as DesError, SeqAccess, Unexpected, Visitor},
        ser::{Error as SerError, Serialize, SerializeStruct, Serializer},
    };
    use serde_big_array::Array;

    use super::*;

    const ZK_PROOF_FIELDS: &'static [&'static str] = &["block_number", "latest_proof"];
    const ZKP_STATE_FIELDS: &'static [&'static str] = &[
        "count",
        "latest_pks",
        "latest_header_hash",
        "latest_block_number",
        "latest_proof",
    ];
    const PROOF_INPUT_FIELDS: &'static [&'static str] = &[
        "block",
        "count",
        "latest_pks",
        "latest_header_hash",
        "previous_proof",
        "genesis_state",
        "prover_keys_path",
    ];

    struct ZKProofVisitor;
    struct ZKPStateVisitor;
    struct ProofInputVisitor;

    impl<'de> Visitor<'de> for ZKProofVisitor {
        type Value = ZKProof;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("struct ZKProof")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let block_number: u32 = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(0, &self))?;
            let latest_ser_proof: Option<Vec<u8>> = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(1, &self))?;

            let latest_proof = if let Some(ser_proof) = latest_ser_proof {
                CanonicalDeserialize::deserialize_compressed(&*ser_proof).map_err(|_| {
                    A::Error::invalid_value(Unexpected::Other("Invalid proof"), &self)
                })?
            } else {
                None
            };

            Ok(ZKProof {
                block_number,
                proof: latest_proof,
            })
        }
    }

    impl Serialize for ZKProof {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut state = serializer.serialize_struct("ZKProof", ZK_PROOF_FIELDS.len())?;
            let ser_latest_proof = if let Some(ref latest_proof) = self.proof {
                let mut writer = Vec::with_capacity(CanonicalSerialize::serialized_size(
                    latest_proof,
                    ark_serialize::Compress::Yes,
                ));
                CanonicalSerialize::serialize_compressed(latest_proof, writer.by_ref())
                    .map_err(|e| S::Error::custom(format!("Could not serialize proof: {}", e)))?;
                Some(writer)
            } else {
                None
            };
            state.serialize_field(ZK_PROOF_FIELDS[0], &self.block_number)?;
            state.serialize_field(ZK_PROOF_FIELDS[1], &ser_latest_proof)?;
            state.end()
        }
    }

    impl<'de> Deserialize<'de> for ZKProof {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_struct("ZKProof", ZK_PROOF_FIELDS, ZKProofVisitor)
        }
    }

    impl<'de> Visitor<'de> for ZKPStateVisitor {
        type Value = ZKPState;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("struct ZKPState")
        }

        /// The deserialization of the ZKPState is unsafe over the network.
        /// It uses unchecked deserialization of elliptic curve points for performance reasons.
        /// We only invoke it when transferring data from the proof generation process.
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let count: usize = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(0, &self))?;
            let ser_latest_pks: Vec<Vec<u8>> = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(1, &self))?;
            let latest_header_hash: Blake2bHash = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(2, &self))?;
            let latest_block_number: u32 = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(3, &self))?;
            let ser_latest_proof: Option<Vec<u8>> = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(4, &self))?;

            let mut latest_pks: Vec<G2MNT6> = vec![];
            for ser_pk in ser_latest_pks.iter().cloned() {
                // Unchecked deserialization happening here.
                latest_pks.push(
                    CanonicalDeserialize::deserialize_uncompressed_unchecked(&*ser_pk).map_err(
                        |_| A::Error::invalid_value(Unexpected::Other("Invalid PK"), &self),
                    )?,
                )
            }
            if latest_pks.len() != count {
                return Err(A::Error::invalid_length(latest_pks.len(), &self));
            }

            let latest_proof = if let Some(ser_proof) = ser_latest_proof {
                CanonicalDeserialize::deserialize_uncompressed_unchecked(&*ser_proof).map_err(
                    |_| A::Error::invalid_value(Unexpected::Other("Invalid proof"), &self),
                )?
            } else {
                None
            };

            Ok(ZKPState {
                latest_pks,
                latest_header_hash,
                latest_block_number,
                latest_proof,
            })
        }
    }

    /// The serialization of the ZKPState is unsafe over the network.
    /// It uses unchecked serialization of elliptic curve points for performance reasons.
    /// We only invoke it when transferring data from the proof generation process.
    impl Serialize for ZKPState {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut ser_latest_pks: Vec<Vec<u8>> = vec![];
            for pk in self.latest_pks.iter() {
                let mut writer = Vec::with_capacity(CanonicalSerialize::uncompressed_size(pk));
                // Unchecked serialization happening here.
                CanonicalSerialize::serialize_uncompressed(pk, writer.by_ref())
                    .map_err(|e| S::Error::custom(format!("Could not serialize pk: {}", e)))?;
                ser_latest_pks.push(writer);
            }
            let ser_latest_proof = if let Some(ref latest_proof) = self.latest_proof {
                let mut writer = Vec::with_capacity(CanonicalSerialize::serialized_size(
                    latest_proof,
                    ark_serialize::Compress::No,
                ));
                CanonicalSerialize::serialize_uncompressed(latest_proof, writer.by_ref())
                    .map_err(|e| S::Error::custom(format!("Could not serialize proof: {}", e)))?;
                Some(writer)
            } else {
                None
            };
            let mut state = serializer.serialize_struct("ZKPState", ZKP_STATE_FIELDS.len())?;
            state.serialize_field(ZKP_STATE_FIELDS[0], &self.latest_pks.len())?;
            state.serialize_field(ZKP_STATE_FIELDS[1], &ser_latest_pks)?;
            state.serialize_field(ZKP_STATE_FIELDS[2], &self.latest_header_hash)?;
            state.serialize_field(ZKP_STATE_FIELDS[3], &self.latest_block_number)?;
            state.serialize_field(ZKP_STATE_FIELDS[4], &ser_latest_proof)?;
            state.end()
        }
    }

    impl<'de> Deserialize<'de> for ZKPState {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_struct("ZKPState", ZKP_STATE_FIELDS, ZKPStateVisitor)
        }
    }

    impl<'de> Visitor<'de> for ProofInputVisitor {
        type Value = ProofInput;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("struct ProofInput")
        }

        /// The deserialization of the ProofInput is unsafe over the network.
        /// It uses unchecked deserialization of elliptic curve points for performance reasons.
        /// We only invoke it when transferring data to the proof generation process.
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let block: MacroBlock = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(0, &self))?;
            let count: usize = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(1, &self))?;
            let ser_latest_pks: Vec<Vec<u8>> = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(2, &self))?;
            let latest_header_hash: Blake2bHash = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(3, &self))?;
            let ser_previous_proof: Option<Vec<u8>> = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(4, &self))?;
            let genesis_state: Array<u8, 95> = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(5, &self))?;
            let path_buf: String = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(6, &self))?;

            let mut latest_pks: Vec<G2MNT6> = vec![];
            for ser_pk in ser_latest_pks.iter().cloned() {
                // Unchecked deserialization happening here.
                latest_pks.push(
                    CanonicalDeserialize::deserialize_uncompressed_unchecked(&*ser_pk).map_err(
                        |_| A::Error::invalid_value(Unexpected::Other("Invalid PK"), &self),
                    )?,
                );
            }

            if latest_pks.len() != count {
                return Err(A::Error::invalid_length(latest_pks.len(), &self));
            }

            let previous_proof = if let Some(ser_proof) = ser_previous_proof {
                Some(
                    CanonicalDeserialize::deserialize_uncompressed_unchecked(&*ser_proof).map_err(
                        |_| A::Error::invalid_value(Unexpected::Other("Invalid proof"), &self),
                    )?,
                )
            } else {
                None
            };

            Ok(ProofInput {
                block,
                latest_pks,
                latest_header_hash,
                previous_proof,
                genesis_state: *genesis_state,
                prover_keys_path: PathBuf::from(path_buf),
            })
        }
    }

    /// The serialization of the ProofInput is unsafe over the network.
    /// It uses unchecked serialization of elliptic curve points for performance reasons.
    /// We only invoke it when transferring data to the proof generation process.
    impl Serialize for ProofInput {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut ser_latest_pks: Vec<Vec<u8>> = vec![];
            for pk in self.latest_pks.iter() {
                let mut writer = Vec::with_capacity(CanonicalSerialize::uncompressed_size(pk));
                // Unchecked serialization happening here.
                CanonicalSerialize::serialize_uncompressed(pk, writer.by_ref())
                    .map_err(|e| S::Error::custom(format!("Could not serialize pk: {}", e)))?;
                ser_latest_pks.push(writer);
            }
            let ser_previous_proof = if let Some(ref previous_proof) = self.previous_proof {
                let mut writer = Vec::with_capacity(CanonicalSerialize::serialized_size(
                    previous_proof,
                    ark_serialize::Compress::No,
                ));
                CanonicalSerialize::serialize_uncompressed(previous_proof, writer.by_ref())
                    .map_err(|e| S::Error::custom(format!("Could not serialize proof: {}", e)))?;
                Some(writer)
            } else {
                None
            };
            let mut state = serializer.serialize_struct("ProofInput", PROOF_INPUT_FIELDS.len())?;
            state.serialize_field(PROOF_INPUT_FIELDS[0], &self.block)?;
            state.serialize_field(PROOF_INPUT_FIELDS[1], &self.latest_pks.len())?;
            state.serialize_field(PROOF_INPUT_FIELDS[2], &ser_latest_pks)?;
            state.serialize_field(PROOF_INPUT_FIELDS[3], &self.latest_header_hash)?;
            state.serialize_field(PROOF_INPUT_FIELDS[4], &ser_previous_proof)?;
            state.serialize_field(PROOF_INPUT_FIELDS[5], &Array(self.genesis_state))?;
            state.serialize_field(
                PROOF_INPUT_FIELDS[6],
                &self.prover_keys_path.to_string_lossy().to_string(),
            )?;
            state.end()
        }
    }

    impl<'de> Deserialize<'de> for ProofInput {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_struct("ProofInput", PROOF_INPUT_FIELDS, ProofInputVisitor)
        }
    }
}
