use std::time::Duration;

use nimiq_network_interface::request::{MessageMarker, RequestCommon};
use nimiq_tendermint::{Step, TaggedAggregationMessage};
use serde::{Deserialize, Serialize};

use super::contribution::AggregateMessage;

/// Self.1 is block height as round and step are already a part of self.0
#[derive(Clone, Debug)]
pub(crate) struct TendermintUpdate(pub TaggedAggregationMessage<AggregateMessage>, pub u32);

impl RequestCommon for TendermintUpdate {
    type Kind = MessageMarker;
    const TYPE_ID: u16 = 124;
    const MAX_REQUESTS: u32 = 500;
    const TIME_WINDOW: std::time::Duration = Duration::from_millis(500);
    type Response = ();
}

impl Serialize for TendermintUpdate {
    fn serialize<W: serde::WriteBytesExt>(
        &self,
        writer: &mut W,
    ) -> Result<usize, serde::SerializingError> {
        let mut size = 0;
        size += Serialize::serialize(&self.0.tag.0, writer)?;
        size += Serialize::serialize(&(self.0.tag.1 as u8), writer)?;
        size += Serialize::serialize(&self.0.aggregation.0, writer)?;
        size += Serialize::serialize(&self.1, writer)?;
        Ok(size)
    }
    fn serialized_size(&self) -> usize {
        // height is u32, round is u32, step is u8
        9 + Serialize::serialized_size(&self.0.aggregation.0)
    }
}

impl Deserialize for TendermintUpdate {
    fn deserialize<R: serde::ReadBytesExt>(
        reader: &mut R,
    ) -> Result<Self, serde::SerializingError> {
        let round = Deserialize::deserialize(reader)?;
        let step: u8 = Deserialize::deserialize(reader)?;
        let step = Step::try_from(step).map_err(|_| serde::SerializingError::InvalidValue)?;
        let aggregation = Deserialize::deserialize(reader)?;
        let height = Deserialize::deserialize(reader)?;
        Ok(TendermintUpdate(
            TaggedAggregationMessage {
                tag: (round, step),
                aggregation: AggregateMessage(aggregation),
            },
            height,
        ))
    }
}
