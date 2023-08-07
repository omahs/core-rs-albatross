use std::{iter, sync::Arc};

use either::Either;
use libp2p::{
    gossipsub,
    identify,
    kad::{store::MemoryStore, Kademlia, KademliaEvent},
    ping::{
        Behaviour as PingBehaviour, Config as PingConfig, Event as PingEvent,
        Failure as PingFailure,
    },
    request_response,
    swarm::{StreamUpgradeError, NetworkBehaviour},
    Multiaddr, PeerId,
};
use nimiq_utils::time::OffsetTime;
use parking_lot::RwLock;

use crate::{
    connection_pool::{
        behaviour::{ConnectionPoolBehaviour, ConnectionPoolEvent},
        handler::ConnectionPoolHandlerError,
    },
    discovery::{
        behaviour::{DiscoveryBehaviour, DiscoveryEvent},
        handler::DiscoveryHandlerError,
        peer_contacts::PeerContactBook,
    },
    dispatch::codecs::typed::{IncomingRequest, MessageCodec, OutgoingResponse, ReqResProtocol},
    Config,
};

pub type NimiqNetworkBehaviourError = Either<
    Either<
        Either<
            Either<
                Either<
                    Either<std::io::Error, DiscoveryHandlerError>,
                    gossipsub::HandlerError,
                >,
                std::io::Error,
            >,
            PingFailure,
        >,
        ConnectionPoolHandlerError,
    >,
    StreamUpgradeError<std::io::Error>,
>;

pub type RequestResponseEvent = request_response::Event<IncomingRequest, OutgoingResponse>;

#[derive(Debug)]
pub enum NimiqEvent {
    Dht(KademliaEvent),
    Discovery(DiscoveryEvent),
    Gossip(gossipsub::Event),
    Identify(identify::Event),
    Ping(PingEvent),
    Pool(ConnectionPoolEvent),
    RequestResponse(RequestResponseEvent),
}

impl From<KademliaEvent> for NimiqEvent {
    fn from(event: KademliaEvent) -> Self {
        Self::Dht(event)
    }
}

impl From<DiscoveryEvent> for NimiqEvent {
    fn from(event: DiscoveryEvent) -> Self {
        Self::Discovery(event)
    }
}

impl From<gossipsub::Event> for NimiqEvent {
    fn from(event: gossipsub::Event) -> Self {
        Self::Gossip(event)
    }
}

impl From<identify::Event> for NimiqEvent {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

impl From<ConnectionPoolEvent> for NimiqEvent {
    fn from(event: ConnectionPoolEvent) -> Self {
        Self::Pool(event)
    }
}

impl From<PingEvent> for NimiqEvent {
    fn from(event: PingEvent) -> Self {
        Self::Ping(event)
    }
}

impl From<RequestResponseEvent> for NimiqEvent {
    fn from(event: RequestResponseEvent) -> Self {
        Self::RequestResponse(event)
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "NimiqEvent")]
pub struct NimiqBehaviour {
    pub dht: Kademlia<MemoryStore>,
    pub discovery: DiscoveryBehaviour,
    pub gossipsub: gossipsub::Behaviour,
    pub identify: identify::Behaviour,
    pub ping: PingBehaviour,
    pub pool: ConnectionPoolBehaviour,
    pub request_response: request_response::Behaviour<MessageCodec>,
}

impl NimiqBehaviour {
    pub fn new(
        config: Config,
        clock: Arc<OffsetTime>,
        contacts: Arc<RwLock<PeerContactBook>>,
        peer_score_params: gossipsub::PeerScoreParams,
    ) -> Self {
        let public_key = config.keypair.public();
        let peer_id = public_key.to_peer_id();

        // DHT behaviour
        let store = MemoryStore::new(peer_id);
        let dht = Kademlia::with_config(peer_id, store, config.kademlia);

        // Discovery behaviour
        let discovery = DiscoveryBehaviour::new(
            config.discovery.clone(),
            config.keypair.clone(),
            Arc::clone(&contacts),
            clock,
        );

        // Gossipsub behaviour
        let thresholds = gossipsub::PeerScoreThresholds::default();
        let mut gossipsub = gossipsub::Behaviour::new(gossipsub::MessageAuthenticity::Author(peer_id), config.gossipsub)
            .expect("Wrong configuration");
        gossipsub
            .with_peer_score(peer_score_params, thresholds)
            .expect("Valid score params and thresholds");

        // Identify behaviour
        let identify_config = identify::Config::new("/albatross/2.0".to_string(), public_key);
        let identify = identify::Behaviour::new(identify_config);

        // Ping behaviour:
        // - Send a ping every 15 seconds and timeout at 20 seconds.
        // - The ping behaviour will close the connection if a ping timeouts.
        let ping = PingBehaviour::new(PingConfig::new());

        // Connection pool behaviour
        let pool = ConnectionPoolBehaviour::new(
            Arc::clone(&contacts),
            peer_id,
            config.seeds,
            config.discovery.required_services,
        );

        // Request Response behaviour
        let codec = MessageCodec::default();
        let protocol = ReqResProtocol::Version1;
        let config = request_response::Config::default();
        let request_response =
            request_response::Behaviour::new(codec, iter::once((protocol, request_response::ProtocolSupport::Full)), config);

        Self {
            dht,
            discovery,
            gossipsub,
            identify,
            ping,
            pool,
            request_response,
        }
    }

    /// Adds a peer address into the DHT
    pub fn add_peer_address(&mut self, peer_id: PeerId, address: Multiaddr) {
        // Add address to the DHT
        self.dht.add_address(&peer_id, address);
    }

    /// Removes a peer from the DHT
    pub fn remove_peer(&mut self, peer_id: PeerId) {
        self.dht.remove_peer(&peer_id);
    }

    /// Removes a peer address from the DHT
    pub fn remove_peer_address(&mut self, peer_id: PeerId, address: Multiaddr) {
        // Remove address from the DHT
        self.dht.remove_address(&peer_id, &address);
    }

    /// Updates the scores of all peers in the peer contact book.
    /// Updates are performed with the score values of Gossipsub
    pub fn update_scores(&self, contacts: Arc<RwLock<PeerContactBook>>) {
        contacts.read().update_scores(&self.gossipsub);
    }
}
