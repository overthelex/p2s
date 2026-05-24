use anyhow::{Context, Result};
use clap::Parser;
use libp2p::Multiaddr;
use p2s_node::{build_swarm, NodeConfig};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::{broadcast, mpsc};

mod cli;
mod event_loop;
mod http_api;
mod keypair_io;
mod metrics;
mod node_state;

use cli::Cli;
use metrics::Metrics;
use node_state::NodeState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "p2s_node=info,libp2p=warn".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    let keypair = keypair_io::load_or_generate(&cli.data_dir)?;
    let peer_id = keypair.public().to_peer_id();

    let mut bootstrap_peers = Vec::new();
    for peer_str in &cli.bootstrap_peer {
        let addr: Multiaddr = peer_str.parse()
            .with_context(|| format!("invalid bootstrap multiaddr: {peer_str}"))?;
        if let Some(peer_id) = extract_peer_id(&addr) {
            bootstrap_peers.push((peer_id, addr));
        } else {
            anyhow::bail!("bootstrap peer must include /p2p/<PeerId>: {peer_str}");
        }
    }

    let validator_keys = cli.validator_key.iter()
        .map(|hex_str| {
            let bytes = hex::decode(hex_str)
                .with_context(|| format!("invalid validator key hex: {hex_str}"))?;
            let bytes: [u8; 32] = bytes.try_into()
                .map_err(|v: Vec<u8>| anyhow::anyhow!("validator key must be 32 bytes, got {}", v.len()))?;
            ed25519_dalek::VerifyingKey::from_bytes(&bytes)
                .context("invalid ed25519 public key")
        })
        .collect::<Result<Vec<_>>>()?;

    let listen_addr: Multiaddr = cli.listen.parse()
        .with_context(|| format!("invalid listen multiaddr: {}", cli.listen))?;

    let config = NodeConfig {
        bootstrap_peers: bootstrap_peers.clone(),
        listen_addr: "0.0.0.0:4001".parse().unwrap(),
        validator_keys,
        replication_factor: 20,
    };

    let mut swarm = build_swarm(keypair, &config)?;
    swarm.listen_on(listen_addr)?;

    for (peer_id, addr) in &bootstrap_peers {
        swarm.behaviour_mut().kademlia.add_address(peer_id, addr.clone());
    }
    if !bootstrap_peers.is_empty() {
        if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
            tracing::warn!("Kademlia bootstrap failed: {e:?}");
        }
    }

    let (cmd_tx, cmd_rx) = mpsc::channel(256);
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let state = Arc::new(NodeState {
        peer_id,
        listen_addrs: Arc::new(RwLock::new(Vec::new())),
        connected_peers: Arc::new(RwLock::new(0)),
        stored_records: Arc::new(RwLock::new(0)),
        cmd_tx,
        pending_queries: Arc::new(Mutex::new(HashMap::new())),
        metrics: Metrics::new(),
    });

    let event_state = state.clone();
    let event_loop_handle = tokio::spawn(async move {
        event_loop::run_event_loop(swarm, cmd_rx, event_state, shutdown_rx).await;
    });

    let router = http_api::build_router(state.clone());
    let http_addr = format!("0.0.0.0:{}", cli.http_port);
    let listener = tokio::net::TcpListener::bind(&http_addr).await
        .with_context(|| format!("failed to bind HTTP on {http_addr}"))?;
    tracing::info!(%http_addr, "HTTP API listening");

    let http_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received");
    let _ = shutdown_tx.send(());

    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), event_loop_handle).await;
    http_handle.abort();

    tracing::info!("Node stopped");
    Ok(())
}

fn extract_peer_id(addr: &Multiaddr) -> Option<libp2p::PeerId> {
    addr.iter().find_map(|proto| {
        if let libp2p::multiaddr::Protocol::P2p(peer_id) = proto {
            Some(peer_id)
        } else {
            None
        }
    })
}
