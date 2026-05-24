use crate::node_state::{NodeState, QueryResult, SwarmCommand};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::oneshot;

pub fn build_router(state: Arc<NodeState>) -> Router {
    Router::new()
        .route("/cards", post(publish_card))
        .route("/cards/{address}", get(fetch_card))
        .route("/health", get(health))
        .route("/node/info", get(node_info))
        .route("/metrics", get(metrics_prometheus))
        .route("/metrics/json", get(metrics_json))
        .with_state(state)
}

#[derive(Deserialize)]
struct PublishCardRequest {
    record: CardRecordJson,
    sig: String,
}

#[derive(Deserialize)]
struct CardRecordJson {
    pubkey: String,
    seq: u64,
    status: String,
    endpoint: String,
    manifest_hash: String,
    domain: String,
    #[serde(default)]
    label: Option<String>,
}

async fn publish_card(
    State(state): State<Arc<NodeState>>,
    Json(req): Json<PublishCardRequest>,
) -> impl IntoResponse {
    let start = Instant::now();
    state.metrics.inc_http_requests();
    state.metrics.inc_put_total();

    let pubkey = match hex::decode(&req.record.pubkey) {
        Ok(b) => b,
        Err(e) => { state.metrics.inc_http_errors(); state.metrics.inc_put_rejected(); return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid pubkey hex: {e}")}))) },
    };
    let sig = match hex::decode(&req.sig) {
        Ok(b) => b,
        Err(e) => { state.metrics.inc_http_errors(); state.metrics.inc_put_rejected(); return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid sig hex: {e}")}))) },
    };
    let manifest_hash = match hex::decode(&req.record.manifest_hash) {
        Ok(b) => b,
        Err(e) => { state.metrics.inc_http_errors(); state.metrics.inc_put_rejected(); return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid manifest_hash hex: {e}")}))) },
    };

    let status = match req.record.status.as_str() {
        "active" => p2s_proto::CardStatus::Active,
        "revoked" => p2s_proto::CardStatus::Revoked,
        _ => { state.metrics.inc_http_errors(); state.metrics.inc_put_rejected(); return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "status must be 'active' or 'revoked'"}))) },
    };

    let signed_card = p2s_proto::SignedCard {
        record: p2s_proto::CardRecord {
            pubkey: pubkey.clone(),
            seq: req.record.seq,
            status,
            endpoint: req.record.endpoint,
            manifest_hash,
            domain: req.record.domain,
            label: req.record.label,
        },
        sig,
    };

    if let Err(e) = p2s_card::verify_card(&signed_card) {
        state.metrics.inc_sig_fail();
        state.metrics.inc_put_rejected();
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("card verification failed: {e}")})));
    }
    state.metrics.inc_sig_ok();

    let address = p2s_card::compute_address(&pubkey);
    let value = match p2s_proto::canonical_encode(&signed_card) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("encoding failed: {e}")}))),
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    let cmd = SwarmCommand::PutRecord { key: address.to_vec(), value, reply: reply_tx };

    if state.cmd_tx.send(cmd).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "node event loop unavailable"})));
    }

    let result = match tokio::time::timeout(std::time::Duration::from_secs(10), reply_rx).await {
        Ok(Ok(QueryResult::PutOk)) => {
            state.metrics.inc_put_success();
            (StatusCode::CREATED, Json(serde_json::json!({"address": hex::encode(address), "status": "published"})))
        }
        Ok(Ok(QueryResult::Error(e))) => {
            state.metrics.inc_put_rejected();
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))
        }
        _ => {
            state.metrics.inc_put_rejected();
            (StatusCode::GATEWAY_TIMEOUT, Json(serde_json::json!({"error": "DHT put timed out"})))
        }
    };

    state.metrics.record_put_latency(start.elapsed().as_micros() as u64);
    result
}

async fn fetch_card(
    State(state): State<Arc<NodeState>>,
    Path(address): Path<String>,
) -> impl IntoResponse {
    let start = Instant::now();
    state.metrics.inc_http_requests();
    state.metrics.inc_get_total();

    let key = match hex::decode(&address) {
        Ok(b) if b.len() == 32 => b,
        Ok(b) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("address must be 32 bytes, got {}", b.len())}))),
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid hex: {e}")}))),
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    let cmd = SwarmCommand::GetRecord { key, reply: reply_tx };

    if state.cmd_tx.send(cmd).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "node event loop unavailable"})));
    }

    let result = match tokio::time::timeout(std::time::Duration::from_secs(10), reply_rx).await {
        Ok(Ok(QueryResult::Found(value))) => {
            state.metrics.inc_get_found();
            match p2s_proto::canonical_decode::<p2s_proto::SignedCard>(&value) {
                Ok(card) => {
                    let resp = serde_json::json!({
                        "record": {
                            "pubkey": hex::encode(&card.record.pubkey),
                            "seq": card.record.seq,
                            "status": match card.record.status { p2s_proto::CardStatus::Active => "active", p2s_proto::CardStatus::Revoked => "revoked" },
                            "endpoint": card.record.endpoint,
                            "manifest_hash": hex::encode(&card.record.manifest_hash),
                            "domain": card.record.domain,
                            "label": card.record.label,
                        },
                        "sig": hex::encode(&card.sig),
                        "address": address,
                    });
                    (StatusCode::OK, Json(resp))
                }
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("decode failed: {e}")}))),
            }
        }
        Ok(Ok(QueryResult::NotFound)) => {
            state.metrics.inc_get_not_found();
            (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "card not found"})))
        }
        Ok(Ok(QueryResult::Error(e))) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))
        }
        _ => {
            (StatusCode::GATEWAY_TIMEOUT, Json(serde_json::json!({"error": "DHT get timed out"})))
        }
    };

    state.metrics.record_get_latency(start.elapsed().as_micros() as u64);
    result
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

async fn node_info(State(state): State<Arc<NodeState>>) -> impl IntoResponse {
    let listen_addrs: Vec<String> = state.listen_addrs.read()
        .map(|addrs| addrs.iter().map(|a| a.to_string()).collect())
        .unwrap_or_default();
    let connected_peers = state.connected_peers.read().map(|c| *c).unwrap_or(0);
    let stored_records = state.stored_records.read().map(|c| *c).unwrap_or(0);

    Json(serde_json::json!({
        "peer_id": state.peer_id.to_string(),
        "listen_addrs": listen_addrs,
        "connected_peers": connected_peers,
        "stored_records": stored_records,
    }))
}

async fn metrics_prometheus(State(state): State<Arc<NodeState>>) -> impl IntoResponse {
    (StatusCode::OK, [("content-type", "text/plain")], state.metrics.render_prometheus())
}

async fn metrics_json(State(state): State<Arc<NodeState>>) -> impl IntoResponse {
    Json(state.metrics.render_json())
}
