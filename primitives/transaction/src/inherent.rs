use crate::reward::RewardTransaction;
use nimiq_hash::{Hash, SerializeContent};
use nimiq_hash_derive::SerializeContent;
use nimiq_keys::Address;
use nimiq_primitives::coin::Coin;
use nimiq_primitives::policy::Policy;
use nimiq_primitives::slots::SlashedSlot;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, SerializeContent, Deserialize)]
#[repr(u8)]
pub enum Inherent {
    Reward { target: Address, value: Coin },
    Slash { slot: SlashedSlot },
    FinalizeBatch,
    FinalizeEpoch,
}

impl Inherent {
    pub fn target(&self) -> &Address {
        match self {
            Inherent::Reward { target, .. } => target,
            Inherent::Slash { .. } | Inherent::FinalizeBatch | Inherent::FinalizeEpoch => {
                &Policy::STAKING_CONTRACT_ADDRESS
            }
        }
    }
}

impl Hash for Inherent {}

impl From<&RewardTransaction> for Inherent {
    fn from(tx: &RewardTransaction) -> Self {
        Self::Reward {
            target: tx.recipient.clone(),
            value: tx.value,
        }
    }
}
