use std::io;

use ark_mnt6_753::Fr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};

use nimiq_hash::{Hash, SerializeContent};
use serde::{Deserialize, ReadBytesExt, Serialize, SerializingError, WriteBytesExt};

use crate::{
    AggregatePublicKey, AggregateSignature, CompressedPublicKey, CompressedSignature, KeyPair,
    PublicKey, SecretKey, Signature,
};

impl Serialize for CompressedPublicKey {
    fn serialize<W: WriteBytesExt>(&self, writer: &mut W) -> Result<usize, SerializingError> {
        assert_eq!(self.as_ref().len(), CompressedPublicKey::SIZE);
        writer.write_all(self.as_ref())?;
        Ok(CompressedPublicKey::SIZE)
    }

    fn serialized_size(&self) -> usize {
        CompressedPublicKey::SIZE
    }
}

impl SerializeContent for CompressedPublicKey {
    fn serialize_content<W: io::Write>(&self, writer: &mut W) -> io::Result<usize> {
        Ok(self.serialize(writer)?)
    }
}

impl Hash for CompressedPublicKey {}

impl std::hash::Hash for CompressedPublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.public_key.to_vec(), state);
    }
}

impl Deserialize for CompressedPublicKey {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        let mut bytes = [0u8; CompressedPublicKey::SIZE];
        reader.read_exact(&mut bytes)?;

        Ok(CompressedPublicKey { public_key: bytes })
    }
}

impl Serialize for CompressedSignature {
    fn serialize<W: WriteBytesExt>(&self, writer: &mut W) -> Result<usize, SerializingError> {
        assert_eq!(self.as_ref().len(), CompressedSignature::SIZE);
        writer.write_all(self.as_ref())?;
        Ok(CompressedSignature::SIZE)
    }

    fn serialized_size(&self) -> usize {
        CompressedSignature::SIZE
    }
}

impl SerializeContent for CompressedSignature {
    fn serialize_content<W: io::Write>(&self, writer: &mut W) -> io::Result<usize> {
        Ok(self.serialize(writer)?)
    }
}

impl Hash for CompressedSignature {}

impl std::hash::Hash for CompressedSignature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.signature.to_vec(), state);
    }
}

impl Deserialize for CompressedSignature {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        let mut bytes = [0u8; CompressedSignature::SIZE];
        reader.read_exact(&mut bytes)?;

        Ok(CompressedSignature { signature: bytes })
    }
}

impl Serialize for PublicKey {
    fn serialize<W: WriteBytesExt>(&self, writer: &mut W) -> Result<usize, SerializingError> {
        self.compress().serialize(writer)
    }

    fn serialized_size(&self) -> usize {
        CompressedPublicKey::SIZE
    }
}

impl SerializeContent for PublicKey {
    fn serialize_content<W: io::Write>(&self, writer: &mut W) -> io::Result<usize> {
        Ok(self.serialize(writer)?)
    }
}

impl Hash for PublicKey {}

impl Deserialize for PublicKey {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        let public_key: CompressedPublicKey = Deserialize::deserialize(reader)?;
        public_key
            .uncompress()
            .map_err(|_| SerializingError::InvalidValue)
    }
}

impl Serialize for Signature {
    fn serialize<W: WriteBytesExt>(&self, writer: &mut W) -> Result<usize, SerializingError> {
        self.compress().serialize(writer)
    }

    fn serialized_size(&self) -> usize {
        CompressedSignature::SIZE
    }
}

impl SerializeContent for Signature {
    fn serialize_content<W: io::Write>(&self, writer: &mut W) -> io::Result<usize> {
        Ok(self.serialize(writer)?)
    }
}

impl Hash for Signature {}

impl Deserialize for Signature {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        let signature: CompressedSignature = Deserialize::deserialize(reader)?;
        signature
            .uncompress()
            .map_err(|_| SerializingError::InvalidValue)
    }
}

impl Serialize for SecretKey {
    fn serialize<W: WriteBytesExt>(&self, writer: &mut W) -> Result<usize, SerializingError> {
        self.secret_key
            .serialize_uncompressed(writer)
            .map_err(ark_to_bserial_error)?;
        Ok(SecretKey::SIZE)
    }

    fn serialized_size(&self) -> usize {
        SecretKey::SIZE
    }
}

impl Deserialize for SecretKey {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        Ok(SecretKey {
            secret_key: Fr::deserialize_uncompressed(reader).map_err(ark_to_bserial_error)?,
        })
    }
}

impl Serialize for AggregatePublicKey {
    fn serialize<W: WriteBytesExt>(&self, writer: &mut W) -> Result<usize, SerializingError> {
        self.0.serialize(writer)
    }

    fn serialized_size(&self) -> usize {
        self.0.serialized_size()
    }
}

impl Deserialize for AggregatePublicKey {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        Ok(AggregatePublicKey(Deserialize::deserialize(reader)?))
    }
}

impl Serialize for AggregateSignature {
    fn serialize<W: WriteBytesExt>(&self, writer: &mut W) -> Result<usize, SerializingError> {
        self.0.serialize(writer)
    }

    fn serialized_size(&self) -> usize {
        self.0.serialized_size()
    }
}

impl Deserialize for AggregateSignature {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        Ok(AggregateSignature(Deserialize::deserialize(reader)?))
    }
}

impl Serialize for KeyPair {
    fn serialize<W: WriteBytesExt>(&self, writer: &mut W) -> Result<usize, SerializingError> {
        self.secret_key.serialize(writer)
    }

    fn serialized_size(&self) -> usize {
        self.secret_key.serialized_size()
    }
}

impl Deserialize for KeyPair {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        let secret: SecretKey = Deserialize::deserialize(reader)?;
        Ok(KeyPair::from(secret))
    }
}

fn ark_to_bserial_error(error: ark_serialize::SerializationError) -> serde::SerializingError {
    match error {
        ark_serialize::SerializationError::NotEnoughSpace => serde::SerializingError::Overflow,
        ark_serialize::SerializationError::InvalidData => serde::SerializingError::InvalidValue,
        ark_serialize::SerializationError::UnexpectedFlags => serde::SerializingError::InvalidValue,
        ark_serialize::SerializationError::IoError(e) => serde::SerializingError::IoError(e),
    }
}
