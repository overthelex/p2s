use ed25519_dalek::VerifyingKey;
use std::net::SocketAddr;
use std::path::PathBuf;

pub struct NodeConfig {
    pub bootstrap_peers: Vec<(libp2p::PeerId, libp2p::Multiaddr)>,
    pub listen_addr: SocketAddr,
    pub validator_keys: Vec<VerifyingKey>,
    pub replication_factor: usize,
    pub data_dir: Option<PathBuf>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            bootstrap_peers: vec![],
            listen_addr: "0.0.0.0:0".parse().unwrap(),
            replication_factor: 20,
            validator_keys: vec![],
            data_dir: None,
        }
    }
}
