use std::fmt::{Debug, Display};
use std::hash::Hash;

use async_trait::async_trait;
use futures::stream::BoxStream;
use thiserror::Error;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use serde::{Deserialize, Serialize};

use crate::{
    peer_info::*,
    request::{Message, Request, RequestError},
};

#[derive(Clone, Debug)]
pub enum NetworkEvent<P> {
    PeerJoined(P, PeerInfo),
    PeerLeft(P),
}

pub type SubscribeEvents<PeerId> =
    BoxStream<'static, Result<NetworkEvent<PeerId>, BroadcastStreamRecvError>>;

pub trait Topic {
    type Item<'de>: Serialize + Deserialize<'de> + Send + Sync + Debug + 'static;

    const BUFFER_SIZE: usize;
    const NAME: &'static str;
    const VALIDATE: bool;
}

// It seems we can't use type aliases on enums yet:
// https://rust-lang.github.io/rfcs/2338-type-alias-enum-variants.html
#[derive(Clone, Debug)]
pub enum MsgAcceptance {
    Accept,
    Reject,
    Ignore,
}

pub trait PubsubId<PeerId>: Clone + Send + Sync + Debug {
    fn propagation_source(&self) -> PeerId;
}

#[derive(Copy, Clone, Debug)]
/// Reasons for closing a connection
pub enum CloseReason {
    /// Reason is unknown or doesn't fit the other reasons
    Other,
    /// The other peer closed the connection
    RemoteClosed,
    /// We need to close the connection to this peer because we are going offline
    /// and don't want new connections.
    GoingOffline,
    /// There was an error and there is need to close the connection
    Error,
    /// Peer is malicious. This will cause the peer ID and address to get banned.
    MaliciousPeer,
}

#[derive(Debug, Error)]
pub enum SendError {
    #[error("{0}")]
    Serialization(#[from] dyn serde::ser::Error),
    #[error("Peer connection already closed")]
    AlreadyClosed,
}

pub trait RequestResponse {
    type Request<'de>: Serialize + Deserialize<'de> + Sync;
    type Response<'de>: Serialize + Deserialize<'de> + Sync;
}

#[async_trait]
pub trait Network: Send + Sync + Unpin + 'static {
    type PeerId: Copy + Debug + Display + Eq + Hash + Send + Sync + Unpin + 'static;
    type AddressType: Debug + Display + 'static;
    type Error: std::error::Error;
    type PubsubId: PubsubId<Self::PeerId> + Send + Sync + Unpin;
    type RequestId: Copy + Debug + Display + Eq + Send + Sync + 'static;

    /// Gets the set of connected peers
    fn get_peers(&self) -> Vec<Self::PeerId>;

    /// Returns wether the current peer has a connection to another peer
    fn has_peer(&self, peer_id: Self::PeerId) -> bool;

    /// Gets a peer information.
    /// If the peer isn't found, `None` is returned.
    fn get_peer_info(&self, peer_id: Self::PeerId) -> Option<PeerInfo>;

    /// Gets the set of connected peers that provide the supplied services.
    /// If we currently don't have min number of connected peer that provides those services,
    /// we dial peers.
    /// If there aren't enough peers in the network that provides the required services, we return an error
    async fn get_peers_by_services(
        &self,
        services: Services,
        min_peers: usize,
    ) -> Result<Vec<Self::PeerId>, Self::Error>;

    /// Returns true when the given peer provides the services flags that are required by us
    fn peer_provides_required_services(&self, peer_id: Self::PeerId) -> bool;

    /// Returns true when the given peer provides the services flags that are required by us
    fn peer_provides_services(&self, peer_id: Self::PeerId, services: Services) -> bool;

    /// Disconnects a peer with a close reason
    async fn disconnect_peer(&self, peer_id: Self::PeerId, close_reason: CloseReason);

    /// Subscribes to network events
    fn subscribe_events(&self) -> SubscribeEvents<Self::PeerId>;

    /// Subscribes to a Gossipsub topic
    async fn subscribe<T, 'de>(
        &self,
    ) -> Result<BoxStream<'static, (T::Item<'de>, Self::PubsubId)>, Self::Error>
    where
        T: Topic + Sync;

    /// Unsubscribes from a Gossipsub topic
    async fn unsubscribe<T>(&self) -> Result<(), Self::Error>
    where
        T: Topic + Sync;

    /// Publishes a message to a Gossipsub topic
    async fn publish<T, 'de>(&self, item: T::Item<'de>) -> Result<(), Self::Error>
    where
        T: Topic + Sync;

    /// Subscribes to a Gossipsub subtopic, providing the subtopic name
    async fn subscribe_subtopic<T, 'de>(
        &self,
        subtopic: String,
    ) -> Result<BoxStream<'static, (T::Item<'de>, Self::PubsubId)>, Self::Error>
    where
        T: Topic + Sync;

    /// Unsubscribes from a Gossipsub subtopic
    async fn unsubscribe_subtopic<T>(&self, subtopic: String) -> Result<(), Self::Error>
    where
        T: Topic + Sync;

    /// Publishes a message to a Gossipsub subtopic
    async fn publish_subtopic<T, 'de>(
        &self,
        subtopic: String,
        item: T::Item<'de>,
    ) -> Result<(), Self::Error>
    where
        T: Topic + Sync;

    /// Validates a message received from a Gossipsub topic
    fn validate_message<T>(&self, id: Self::PubsubId, acceptance: MsgAcceptance)
    where
        T: Topic + Sync;

    /// Gets a value from the distributed hash table
    async fn dht_get<K, V, 'de>(&self, k: &K) -> Result<Option<V>, Self::Error>
    where
        K: AsRef<[u8]> + Send + Sync,
        V: Deserialize<'de> + Send + Sync;

    /// Puts a value to the distributed hash table
    async fn dht_put<K, V>(&self, k: &K, v: &V) -> Result<(), Self::Error>
    where
        K: AsRef<[u8]> + Send + Sync,
        V: Serialize + Send + Sync;

    /// Dials a peer
    async fn dial_peer(&self, peer_id: Self::PeerId) -> Result<(), Self::Error>;

    /// Dials an address
    async fn dial_address(&self, address: Self::AddressType) -> Result<(), Self::Error>;

    /// Gets the local peer ID
    fn get_local_peer_id(&self) -> Self::PeerId;

    /// Sends a message to a specific peer
    async fn message<'a, M: Message<'a>>(
        &self,
        request: M,
        peer_id: Self::PeerId,
    ) -> Result<(), RequestError>;

    /// Requests data from a specific peer
    async fn request<'a, Req: Request<'a>>(
        &self,
        request: Req,
        peer_id: Self::PeerId,
    ) -> Result<Req::Response, RequestError>;

    /// Receives messages from peers.
    /// This function returns a stream where the messages are going to be propagated.
    fn receive_messages<'a, M: Message<'a>>(&self) -> BoxStream<'static, (M, Self::PeerId)>;

    /// Receives requests from peers.
    /// This function returns a stream where the requests are going to be propagated.
    fn receive_requests<'a, Req: Request<'a>>(
        &self,
    ) -> BoxStream<'static, (Req, Self::RequestId, Self::PeerId)>;

    /// Sends a response to a specific request
    async fn respond<'a, Req: Request<'a>>(
        &self,
        request_id: Self::RequestId,
        response: Req::Response,
    ) -> Result<(), Self::Error>;
}
