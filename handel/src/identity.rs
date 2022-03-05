use bls::PublicKey;
use collections::bitset::BitSet;

use crate::contribution::AggregatableContribution;

#[derive(Clone, std::fmt::Debug)]
pub enum Identity {
    Single(usize),
    Multiple(Vec<usize>),
    None,
}

impl Identity {
    pub fn as_bitset(&self) -> BitSet {
        let mut bitset = BitSet::new();
        match self {
            Self::Single(id) => bitset.insert(*id),
            Self::Multiple(ids) => ids.iter().for_each(|id| bitset.insert(*id)),
            Self::None => {}
        }
        bitset
    }

    pub fn len(&self) -> usize {
        match self {
            Identity::None => 0,
            Identity::Single(_) => 1,
            Identity::Multiple(ids) => ids.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub trait IdentityRegistry: Send + Sync {
    fn public_key(&self, id: usize) -> Option<PublicKey>;

    fn signers_identity(&self, signers: &BitSet) -> Identity;
}

pub trait WeightRegistry: Send + Sync {
    fn weight(&self, id: usize) -> Option<usize>;

    fn signers_weight(&self, signers: &BitSet) -> Option<usize> {
        let mut votes = 0;
        for signer in signers.iter() {
            votes += self.weight(signer)?;
        }
        Some(votes)
    }

    fn signature_weight<C: AggregatableContribution>(&self, contribution: &C) -> Option<usize> {
        self.signers_weight(&contribution.contributors())
    }
}

pub trait ThresholdEvaluator<C: AggregatableContribution>: WeightRegistry {
    /// Function used to determine after what threshold the aggregating stops and is
    /// replaced by simply returning the conclusive yet not full aggregation to senders of contributions.
    fn is_threshold_reached(&self, contribution: &C) -> bool;
}
