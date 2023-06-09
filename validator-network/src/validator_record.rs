use nimiq_bls::{PublicKey, SecretKey, Signature};
use nimiq_utils::tagged_signing::TaggedSignable;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

// TODO: Use a tagged signature for validator records
impl<TPeerId> TaggedSignable for ValidatorRecord<TPeerId>
where
    TPeerId: Serialize + DeserializeOwned,
{
    const TAG: u8 = 0x03;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "TPeerId: Serialize + DeserializeOwned")]
pub struct ValidatorRecord<TPeerId>
where
    TPeerId: Serialize + DeserializeOwned,
{
    pub peer_id: TPeerId,
    // TODO: other info, like public key?
}

impl<TPeerId> ValidatorRecord<TPeerId>
where
    TPeerId: Serialize + DeserializeOwned,
{
    pub fn new(peer_id: TPeerId) -> Self {
        Self { peer_id }
    }

    pub fn sign(self, secret_key: &SecretKey) -> SignedValidatorRecord<TPeerId> {
        let data =
            postcard::to_allocvec(&self).expect("Could not serialize signed validator record");
        let signature = secret_key.sign(&data);

        SignedValidatorRecord {
            record: self,
            signature,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "TPeerId: Serialize + DeserializeOwned")]
pub struct SignedValidatorRecord<TPeerId>
where
    TPeerId: Serialize + DeserializeOwned,
{
    pub record: ValidatorRecord<TPeerId>,
    pub signature: Signature,
}

impl<TPeerId> SignedValidatorRecord<TPeerId>
where
    TPeerId: Serialize + DeserializeOwned,
{
    pub fn verify(&self, public_key: &PublicKey) -> bool {
        public_key.verify(
            &postcard::to_allocvec(&self.record).expect("Could not serialize record"),
            &self.signature,
        )
    }
}
