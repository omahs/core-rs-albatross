use nimiq_bls::{PublicKey, SecretKey, Signature};
use nimiq_utils::tagged_signing::TaggedSignable;
use serde::{Deserialize, Serialize};

// TODO: Use a tagged signature for validator records
impl<TPeerId> TaggedSignable for ValidatorRecord<TPeerId>
where
    TPeerId: Serialize + Deserialize,
{
    const TAG: u8 = 0x03;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidatorRecord<TPeerId>
where
    TPeerId: Serialize + Deserialize,
{
    pub peer_id: TPeerId,
    // TODO: other info, like public key?
}

impl<TPeerId> ValidatorRecord<TPeerId>
where
    TPeerId: Serialize + Deserialize,
{
    pub fn new(peer_id: TPeerId) -> Self {
        Self { peer_id }
    }

    pub fn sign(self, secret_key: &SecretKey) -> SignedValidatorRecord<TPeerId> {
        let data = self.serialize_to_vec();
        let signature = secret_key.sign(&data);

        SignedValidatorRecord {
            record: self,
            signature,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedValidatorRecord<TPeerId>
where
    TPeerId: Serialize + Deserialize,
{
    pub record: ValidatorRecord<TPeerId>,
    pub signature: Signature,
}

impl<TPeerId> SignedValidatorRecord<TPeerId>
where
    TPeerId: Serialize + Deserialize,
{
    pub fn verify(&self, public_key: &PublicKey) -> bool {
        public_key.verify(&self.record.serialize_to_vec(), &self.signature)
    }
}
