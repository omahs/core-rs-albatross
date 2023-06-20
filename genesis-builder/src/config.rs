use std::convert::TryFrom;

use nimiq_bls::PublicKey as BlsPublicKey;
use nimiq_keys::{Address, PublicKey as SchnorrPublicKey};
use nimiq_primitives::coin::Coin;
use nimiq_vrf::VrfSeed;
use serde::{de::Error, Deserialize, Deserializer};
use time::OffsetDateTime;

#[derive(Clone, Debug, Deserialize)]
pub struct GenesisConfig {
    #[serde(default)]
    pub seed_message: Option<String>,

    pub vrf_seed: Option<VrfSeed>,

    #[serde(deserialize_with = "deserialize_timestamp")]
    pub timestamp: Option<OffsetDateTime>,

    #[serde(default)]
    pub validators: Vec<GenesisValidator>,

    #[serde(default)]
    pub stakers: Vec<GenesisStaker>,

    #[serde(default)]
    pub accounts: Vec<GenesisAccount>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GenesisValidator {
    pub validator_address: Address,

    pub signing_key: SchnorrPublicKey,

    pub voting_key: BlsPublicKey,

    pub reward_address: Address,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GenesisStaker {
    pub staker_address: Address,

    #[serde(deserialize_with = "deserialize_coin")]
    pub balance: Coin,

    pub delegation: Address,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GenesisAccount {
    pub address: Address,

    #[serde(deserialize_with = "deserialize_coin")]
    pub balance: Coin,
}

pub(crate) fn deserialize_coin<'de, D>(deserializer: D) -> Result<Coin, D::Error>
where
    D: Deserializer<'de>,
{
    let value: u64 = Deserialize::deserialize(deserializer)?;
    Coin::try_from(value).map_err(Error::custom)
}

pub fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<Option<OffsetDateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Deserialize::deserialize(deserializer)?;
    if let Some(s) = opt {
        Ok(Some(
            OffsetDateTime::parse(&s, &time::format_description::well_known::Rfc3339)
                .map_err(|e| Error::custom(format!("{e:?}")))?,
        ))
    } else {
        Ok(None)
    }
}
