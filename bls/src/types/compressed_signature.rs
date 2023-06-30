use std::{
    cmp::Ordering,
    fmt,
    io::{Error, ErrorKind},
};

use ark_ec::{AffineRepr, CurveGroup};
use ark_mnt6_753::{G1Affine, G1Projective};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};

use crate::Signature;

/// The serialized compressed form of a signature.
/// This form consists of the x-coordinate of the point (in the affine form),
/// one bit indicating the sign of the y-coordinate
/// and one bit indicating if it is the "point-at-infinity".
#[derive(Clone, Copy)]
#[cfg_attr(feature = "serde-derive", derive(serde::Serialize, serde::Deserialize))]
pub struct CompressedSignature {
    #[cfg_attr(feature = "serde-derive", serde(with = "nimiq_serde::HexArray"))]
    pub signature: [u8; 95],
}

impl CompressedSignature {
    pub const SIZE: usize = 95;

    /// Transforms the compressed form back into the projective form.
    pub fn uncompress(&self) -> Result<Signature, Error> {
        let affine_point: G1Affine =
            CanonicalDeserialize::deserialize_compressed(&mut &self.signature[..])
                .map_err(|e| Error::new(ErrorKind::Other, e))?;
        let signature = affine_point.into_group();
        Ok(Signature {
            signature,
            compressed: *self,
        })
    }

    /// Formats the compressed form into a hexadecimal string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.as_ref())
    }
}

impl Eq for CompressedSignature {}

impl PartialEq for CompressedSignature {
    fn eq(&self, other: &CompressedSignature) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Ord for CompressedSignature {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl PartialOrd<CompressedSignature> for CompressedSignature {
    fn partial_cmp(&self, other: &CompressedSignature) -> Option<Ordering> {
        self.as_ref().partial_cmp(other.as_ref())
    }
}

impl Default for CompressedSignature {
    fn default() -> Self {
        CompressedSignature {
            signature: [0u8; CompressedSignature::SIZE],
        }
    }
}

impl fmt::Debug for CompressedSignature {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "CompressedSignature({})", self.to_hex())
    }
}

impl fmt::Display for CompressedSignature {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.to_hex())
    }
}

impl From<G1Projective> for CompressedSignature {
    fn from(signature: G1Projective) -> Self {
        let mut buffer = [0u8; CompressedSignature::SIZE];
        CanonicalSerialize::serialize_compressed(&signature.into_affine(), &mut &mut buffer[..])
            .unwrap();
        CompressedSignature { signature: buffer }
    }
}

impl AsRef<[u8]> for CompressedSignature {
    fn as_ref(&self) -> &[u8] {
        self.signature.as_ref()
    }
}

impl AsMut<[u8]> for CompressedSignature {
    fn as_mut(&mut self) -> &mut [u8] {
        self.signature.as_mut()
    }
}

impl std::hash::Hash for CompressedSignature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.signature.to_vec(), state);
    }
}

#[cfg(feature = "serde-derive")]
mod serde_derive {
    // TODO: Replace this with a generic serialization using `ToHex` and `FromHex`.

    use std::{io, str::FromStr};

    use nimiq_hash::SerializeContent;

    use super::CompressedSignature;
    use crate::ParseError;

    impl FromStr for CompressedSignature {
        type Err = ParseError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let raw = hex::decode(s)?;
            if raw.len() != CompressedSignature::SIZE {
                return Err(ParseError::IncorrectLength(raw.len()));
            }
            postcard::from_bytes(&raw).map_err(|_| ParseError::SerializationError)
        }
    }

    impl SerializeContent for CompressedSignature {
        fn serialize_content<W: io::Write, H>(&self, writer: &mut W) -> io::Result<usize> {
            let s =
                postcard::to_allocvec(self).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            writer.write_all(&s)?;
            Ok(s.len())
        }
    }
}
