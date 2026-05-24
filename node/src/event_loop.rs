use crate::node_state::{NodeState, PendingQuery, QueryResult, SwarmCommand};
use libp2p::futures::StreamExt;
use libp2p::identify;
use libp2p::kad::store::RecordStore;
use libp2p::kad::{self, QueryResult as KadQueryResult};
use libp2p::swarm::SwarmEvent;
use libp2p::Swarm;
use p2s_node::{NodeBehaviour, NodeBehaviourEvent};
use std::sync::Arc;
use tokio::sync::mpsc;

pub async fn run_event_loop(
    mut swarm: Swarm<NodeBehaviour>,
    mut cmd_rx: mpsc::Receiver<SwarmCommand>,
    state: Arc<NodeState>,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            event = swarm.select_next_some() => {
                handle_swarm_event(event, &mut swarm, &state);
            }
            Some(cmd) = cmd_rx.recv() => {
                handle_command(cmd, &mut swarm, &state);
            }
            _ = shutdown.recv() => {
                tracing::info!("Shutting down swarm event loop");
                break;
            }
        }
    }
}

fn handle_swarm_event(
    event: SwarmEvent<NodeBehaviourEvent>,
    swarm: &mut Swarm<NodeBehaviour>,
    state: &Arc<NodeState>,
) {
    match event {
        SwarmEvent::Behaviour(NodeBehaviourEvent::Kademlia(kad_event)) => {
            handle_kad_event(kad_event, state);
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Identify(identify::Event::Received {
            peer_id,
            info,
            ..
        })) => {
            tracing::info!(%peer_id, protocols = ?info.protocols.len(), addrs = ?info.listen_addrs.len(), "Identify: peer discovered");
            for addr in &info.listen_addrs {
                swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
            }
            state.metrics.set_peers(swarm.connected_peers().count() as u64);
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Identify(_)) => {}
        SwarmEvent::NewListenAddr { address, .. } => {
            tracing::info!(%address, "Listening on");
            if let Ok(mut addrs) = state.listen_addrs.write() {
                addrs.push(address);
            }
        }
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            tracing::debug!(%peer_id, "Connection established");
            if let Ok(mut count) = state.connected_peers.write() {
                *count += 1;
            }
            state.metrics.set_peers(swarm.connected_peers().count() as u64);
        }
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
            tracing::debug!(%peer_id, "Connection closed");
            if let Ok(mut count) = state.connected_peers.write() {
                *count = count.saturating_sub(1);
            }
            state.metrics.set_peers(swarm.connected_peers().count() as u64);
        }
        _ => {}
    }
}

fn handle_kad_event(event: kad::Event, state: &Arc<NodeState>) {
    match event {
        kad::Event::OutboundQueryProgressed { id, result, .. } => {
            match result {
                KadQueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(peer_record))) => {
                    if let Ok(mut pending) = state.pending_queries.lock() {
                        if let Some(pq) = pending.remove(&id) {
                            let _ = pq.reply.send(QueryResult::Found(peer_record.record.value));
                        }
                    }
                }
                KadQueryResult::GetRecord(Ok(kad::GetRecordOk::FinishedWithNoAdditionalRecord { .. })) => {
                    if let Ok(mut pending) = state.pending_queries.lock() {
                        if let Some(pq) = pending.remove(&id) {
                            let _ = pq.reply.send(QueryResult::NotFound);
                        }
                    }
                }
                KadQueryResult::GetRecord(Err(e)) => {
                    if let Ok(mut pending) = state.pending_queries.lock() {
                        if let Some(pq) = pending.remove(&id) {
                            let _ = pq.reply.send(QueryResult::NotFound);
                        }
                    }
                    tracing::debug!(?e, "Kademlia GET failed");
                }
                KadQueryResult::PutRecord(Ok(_)) => {
                    if let Ok(mut pending) = state.pending_queries.lock() {
                        if let Some(pq) = pending.remove(&id) {
                            let _ = pq.reply.send(QueryResult::PutOk);
                        }
                    }
                }
                KadQueryResult::PutRecord(Err(_)) => {
                    if let Ok(mut pending) = state.pending_queries.lock() {
                        if let Some(pq) = pending.remove(&id) {
                            let _ = pq.reply.send(QueryResult::PutOk);
                        }
                    }
                }
                KadQueryResult::Bootstrap(Ok(result)) => {
                    tracing::info!(num_remaining = result.num_remaining, "Kademlia bootstrap progress");
                }
                KadQueryResult::Bootstrap(Err(e)) => {
                    tracing::warn!(?e, "Kademlia bootstrap failed");
                }
                _ => {}
            }
        }
        kad::Event::RoutingUpdated { peer, .. } => {
            tracing::debug!(%peer, "Routing table updated");
        }
        _ => {}
    }
}

fn handle_command(cmd: SwarmCommand, swarm: &mut Swarm<NodeBehaviour>, state: &Arc<NodeState>) {
    match cmd {
        SwarmCommand::PutRecord { key, value, reply } => {
            let record = libp2p::kad::Record {
                key: libp2p::kad::RecordKey::new(&key),
                value,
                publisher: None,
                expires: None,
            };
            let store_result = swarm.behaviour_mut().kademlia
                .store_mut()
                .put(record.clone());

            match store_result {
                Ok(()) => {
                    if let Ok(mut count) = state.stored_records.write() {
                        *count += 1;
                    }
                    state.metrics.set_records(state.stored_records.read().map(|c| *c as u64).unwrap_or(0));
                    match swarm.behaviour_mut().kademlia.put_record(record, libp2p::kad::Quorum::One) {
                        Ok(query_id) => {
                            if let Ok(mut pending) = state.pending_queries.lock() {
                                pending.insert(query_id, PendingQuery { reply });
                            }
                        }
                        Err(_) => {
                            let _ = reply.send(QueryResult::PutOk);
                        }
                    }
                }
                Err(e) => {
                    let _ = reply.send(QueryResult::Error(format!("{e:?}")));
                }
            }
        }
        SwarmCommand::GetRecord { key, reply } => {
            // Try local store first before going to DHT
            let record_key = libp2p::kad::RecordKey::new(&key);
            if let Some(record) = swarm.behaviour_mut().kademlia.store_mut().get(&record_key) {
                let _ = reply.send(QueryResult::Found(record.into_owned().value));
                return;
            }
            let query_id = swarm.behaviour_mut().kademlia.get_record(record_key);
            if let Ok(mut pending) = state.pending_queries.lock() {
                pending.insert(query_id, PendingQuery { reply });
            }
        }
    }
}
