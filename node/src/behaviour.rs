use crate::config::NodeConfig;
use crate::record_store::CardRecordStore;
use libp2p::{
    identity,
    kad,
    noise, tcp, yamux, Swarm, SwarmBuilder,
};
use std::time::Duration;

#[derive(libp2p::swarm::NetworkBehaviour)]
pub struct NodeBehaviour {
    pub kademlia: kad::Behaviour<CardRecordStore>,
    pub identify: libp2p::identify::Behaviour,
}

const MAX_RECORDS: usize = 100_000;
const PROTOCOL_NAME: libp2p::StreamProtocol =
    libp2p::StreamProtocol::new("/p2s/kad/1.0.0");

pub fn build_swarm(
    keypair: identity::Keypair,
    config: &NodeConfig,
) -> anyhow::Result<Swarm<NodeBehaviour>> {
    let peer_id = keypair.public().to_peer_id();

    let store = CardRecordStore::new(MAX_RECORDS);

    let mut kad_config = kad::Config::new(PROTOCOL_NAME);
    kad_config.set_replication_factor(
        std::num::NonZero::new(config.replication_factor).unwrap()
    );
    kad_config.set_record_ttl(None);
    kad_config.set_provider_record_ttl(None);

    let kademlia = kad::Behaviour::with_config(peer_id, store, kad_config);

    let identify = libp2p::identify::Behaviour::new(
        libp2p::identify::Config::new("/p2s/id/1.0.0".into(), keypair.public())
            .with_push_listen_addr_updates(true),
    );

    let behaviour = NodeBehaviour { kademlia, identify };

    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_| Ok(behaviour))?
        .with_swarm_config(|cfg| {
            cfg.with_idle_connection_timeout(Duration::from_secs(60))
        })
        .build();

    Ok(swarm)
}
