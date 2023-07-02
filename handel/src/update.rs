use std::fmt::Debug;

use nimiq_serde::fixint;
use serde::{Deserialize, Serialize};

use crate::contribution::AggregatableContribution;
/// The max number of LevelUpdateMessages requests per peer.
pub const MAX_REQUEST_RESPONSE_LEVEL_UPDATE_MESSAGE: u32 = 1000;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "C: AggregatableContribution")]
pub struct LevelUpdate<C: AggregatableContribution> {
    /// The updated multi-signature for this level
    pub aggregate: C,

    /// The individual signature of the sender, or `None`
    pub(crate) individual: Option<C>,

    /// The level to which this multi-signature belongs to
    pub(crate) level: u8,

    /// The validator ID of the sender (a.k.a. `pk_idx`)
    ///
    /// NOTE: It's safe to just send your own validator ID, since everything critical is authenticated
    /// by signatures anyway.
    #[serde(with = "fixint::be")]
    pub(crate) origin: u16,
}

impl<C: AggregatableContribution> LevelUpdate<C> {
    /// Create a new LevelUpdate
    /// * `aggregate` - The aggregated contribution
    /// * `individual` - The contribution of the sender, or none. Must have `individual.num_contributors() == 1`
    /// * `level` - The level this update belongs to
    /// * `origin` - the identifier of the sender
    pub fn new(aggregate: C, individual: Option<C>, level: usize, origin: usize) -> Self {
        Self {
            aggregate,
            individual,
            level: level as u8,
            origin: origin as u16,
        }
    }

    /// The source (i.e id) of the sender of this update
    pub fn origin(&self) -> usize {
        self.origin as usize
    }

    /// Returns the level this update is for
    pub fn level(&self) -> usize {
        self.level as usize
    }
}
