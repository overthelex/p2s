use ed25519_dalek::VerifyingKey;
use std::net::SocketAddr;

pub struct NodeConfig {
    /// Addresses of bootstrap nodes to connect to on startup.
    pub bootstrap_peers: Vec<(libp2p::PeerId, libp2p::Multiaddr)>,

    /// Address to listen on.
    pub listen_addr: SocketAddr,

    /// Trusted validator public keys for mandate verification.
    pub validator_keys: Vec<VerifyingKey>,

    /// Kademlia replication factor (k).
    pub replication_factor: usize,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            bootstrap_peers: vec![],
            listen_addr: "0.0.0.0:0".parse().unwrap(),
            replication_factor: 20,
            validator_keys: vec![],
        }
    }
}
