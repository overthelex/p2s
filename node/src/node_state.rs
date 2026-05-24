use crate::metrics::Metrics;
use libp2p::{Multiaddr, PeerId};
use p2s_node::RateLimiter;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::{mpsc, oneshot};
use libp2p::kad::QueryId;

pub struct NodeState {
    pub peer_id: PeerId,
    pub listen_addrs: Arc<RwLock<Vec<Multiaddr>>>,
    pub connected_peers: Arc<RwLock<usize>>,
    pub stored_records: Arc<RwLock<usize>>,
    pub cmd_tx: mpsc::Sender<SwarmCommand>,
    pub pending_queries: Arc<Mutex<HashMap<QueryId, PendingQuery>>>,
    pub metrics: Metrics,
    pub rate_limiter: Mutex<RateLimiter>,
}

pub struct PendingQuery {
    pub reply: oneshot::Sender<QueryResult>,
}

pub enum SwarmCommand {
    PutRecord {
        key: Vec<u8>,
        value: Vec<u8>,
        reply: oneshot::Sender<QueryResult>,
    },
    GetRecord {
        key: Vec<u8>,
        reply: oneshot::Sender<QueryResult>,
    },
}

pub enum QueryResult {
    PutOk,
    Found(Vec<u8>),
    NotFound,
    Error(String),
}
