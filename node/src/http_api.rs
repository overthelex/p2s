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
        .route("/cards/batch", post(publish_batch))
        .route("/cards/{address}", get(fetch_card))
        .route("/health", get(health))
        .route("/node/info", get(node_info))
        .route("/metrics", get(metrics_prometheus))
        .route("/metrics/json", get(metrics_json))
        .route("/appeals", post(submit_appeal))
        .route("/admin/reviews", get(list_reviews))
        .route("/admin/reviews/{address}", post(resolve_review))
        .with_state(state)
}

#[derive(Deserialize)]
struct PublishCardRequest {
    record: CardRecordJson,
    sig: String,
    #[serde(default)]
    challenge_nonce: Option<String>,
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

    let card_for_verify = signed_card.clone();
    let verify_result = tokio::task::spawn_blocking(move || {
        p2s_card::verify_card(&card_for_verify)
    }).await;
    match verify_result {
        Ok(Ok(())) => { state.metrics.inc_sig_ok(); }
        Ok(Err(e)) => {
            state.metrics.inc_sig_fail();
            state.metrics.inc_put_rejected();
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("card verification failed: {e}")})));
        }
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "verification task failed"})));
        }
    }

    // Rate limiting per domain
    if let Ok(limiter) = state.rate_limiter.lock() {
        match limiter.check(&signed_card.record.domain) {
            p2s_node::RateLimitResult::TooManyCards { domain, max, .. } => {
                state.metrics.inc_put_rejected();
                return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({"error": format!("rate limit: domain {domain} has reached max {max} cards")})));
            }
            p2s_node::RateLimitResult::CooldownActive { domain, remaining_secs } => {
                state.metrics.inc_put_rejected();
                return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({"error": format!("rate limit: domain {domain} cooldown, retry in {remaining_secs}s")})));
            }
            p2s_node::RateLimitResult::Allowed => {}
        }
    }

    // §1.4 Free-text field hardening (label)
    if let Some(ref label) = signed_card.record.label {
        if let Err(e) = p2s_verifier::harden_label(label) {
            state.metrics.inc_put_rejected();
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid label: {e}")})));
        }
    }

    // §1.3 Endpoint URL validation
    if let Err(e) = p2s_verifier::validate_endpoint(&signed_card.record.endpoint) {
        state.metrics.inc_put_rejected();
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid endpoint: {e}")})));
    }

    // §1.1 Domain ownership (if challenge_nonce provided)
    let mut trust_weight: Option<f64> = None;
    if let Some(ref nonce_hex) = req.challenge_nonce {
        let nonce_bytes = match hex::decode(nonce_hex) {
            Ok(b) if b.len() == 16 => {
                let mut arr = [0u8; 16];
                arr.copy_from_slice(&b);
                arr
            }
            Ok(b) => {
                state.metrics.inc_put_rejected();
                return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("challenge_nonce must be 16 bytes, got {}", b.len())})));
            }
            Err(e) => {
                state.metrics.inc_put_rejected();
                return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid challenge_nonce hex: {e}")})));
            }
        };

        match p2s_verifier::run_stage1(&signed_card, &nonce_bytes).await {
            p2s_verifier::Stage1Outcome::Passed(facts) => {
                trust_weight = Some(if facts.domain_verified { 1.0 } else { 0.0 });
            }
            p2s_verifier::Stage1Outcome::Rejected { step, reason } => {
                state.metrics.inc_put_rejected();
                return (StatusCode::FORBIDDEN, Json(serde_json::json!({
                    "error": format!("verification failed at {step}: {reason}")
                })));
            }
        }
    }

    let card_domain = signed_card.record.domain.clone();
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
            if let Ok(mut limiter) = state.rate_limiter.lock() {
                limiter.record_publish(&card_domain);
            }
            let addr_hex = hex::encode(address);
            if let Some(w) = trust_weight {
                if let Ok(mut weights) = state.trust_weights.lock() {
                    weights.set(&addr_hex, w);
                }
            }
            (StatusCode::CREATED, Json(serde_json::json!({
                "address": addr_hex,
                "status": "published",
                "trust_weight": trust_weight,
            })))
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

async fn publish_batch(
    State(state): State<Arc<NodeState>>,
    Json(cards): Json<Vec<PublishCardRequest>>,
) -> impl IntoResponse {
    let mut results = Vec::with_capacity(cards.len());
    let mut ok = 0u64;
    let mut fail = 0u64;

    for req in cards {
        state.metrics.inc_http_requests();
        state.metrics.inc_put_total();
        let start = std::time::Instant::now();

        let pubkey = match hex::decode(&req.record.pubkey) { Ok(b) => b, Err(_) => { fail += 1; state.metrics.inc_put_rejected(); continue } };
        let sig = match hex::decode(&req.sig) { Ok(b) => b, Err(_) => { fail += 1; state.metrics.inc_put_rejected(); continue } };
        let manifest_hash = match hex::decode(&req.record.manifest_hash) { Ok(b) => b, Err(_) => { fail += 1; state.metrics.inc_put_rejected(); continue } };
        let status = match req.record.status.as_str() { "active" => p2s_proto::CardStatus::Active, "revoked" => p2s_proto::CardStatus::Revoked, _ => { fail += 1; state.metrics.inc_put_rejected(); continue } };

        let signed_card = p2s_proto::SignedCard {
            record: p2s_proto::CardRecord { pubkey: pubkey.clone(), seq: req.record.seq, status, endpoint: req.record.endpoint, manifest_hash, domain: req.record.domain, label: req.record.label },
            sig,
        };

        if p2s_card::verify_card(&signed_card).is_err() {
            fail += 1; state.metrics.inc_sig_fail(); state.metrics.inc_put_rejected(); continue;
        }
        state.metrics.inc_sig_ok();

        let address = p2s_card::compute_address(&pubkey);
        let value = match p2s_proto::canonical_encode(&signed_card) { Ok(v) => v, Err(_) => { fail += 1; continue } };

        let (reply_tx, reply_rx) = oneshot::channel();
        let cmd = SwarmCommand::PutRecord { key: address.to_vec(), value, reply: reply_tx };
        if state.cmd_tx.send(cmd).await.is_err() { fail += 1; continue; }

        match tokio::time::timeout(std::time::Duration::from_secs(10), reply_rx).await {
            Ok(Ok(QueryResult::PutOk)) => {
                ok += 1;
                state.metrics.inc_put_success();
                if let Ok(mut limiter) = state.rate_limiter.lock() {
                    limiter.record_publish(&signed_card.record.domain);
                }
                results.push(serde_json::json!({"address": hex::encode(address), "status": "published"}));
            }
            _ => { fail += 1; state.metrics.inc_put_rejected(); }
        }

        state.metrics.record_put_latency(start.elapsed().as_micros() as u64);
    }

    (StatusCode::OK, Json(serde_json::json!({"ok": ok, "fail": fail, "results": results})))
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
                    let weight = state.trust_weights.lock()
                        .ok()
                        .and_then(|w| w.get(&address));
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
                        "trust_weight": weight,
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

#[derive(Deserialize)]
struct AppealRequest {
    address: String,
    reason: String,
}

async fn submit_appeal(
    State(state): State<Arc<NodeState>>,
    Json(req): Json<AppealRequest>,
) -> impl IntoResponse {
    let Some(ref queue) = state.review_queue else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "review queue not available"})),
        );
    };

    let current_weight = state
        .trust_weights
        .lock()
        .ok()
        .and_then(|w| w.get(&req.address))
        .unwrap_or(1.0);

    match queue.submit(&req.address, req.reason, current_weight) {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({"status": "appeal_submitted", "address": req.address})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("failed to submit appeal: {e}")})),
        ),
    }
}

async fn list_reviews(State(state): State<Arc<NodeState>>) -> impl IntoResponse {
    let Some(ref queue) = state.review_queue else {
        return Json(serde_json::json!({"reviews": []}));
    };

    let reviews = queue.list_pending();
    Json(serde_json::json!({"reviews": reviews}))
}

#[derive(Deserialize)]
struct ResolveReviewRequest {
    #[serde(default)]
    new_weight: Option<f64>,
}

async fn resolve_review(
    State(state): State<Arc<NodeState>>,
    Path(address): Path<String>,
    Json(req): Json<ResolveReviewRequest>,
) -> impl IntoResponse {
    let Some(ref queue) = state.review_queue else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "review queue not available"})),
        );
    };

    if let Some(new_weight) = req.new_weight {
        let clamped = new_weight.clamp(0.0, 1.0);
        if let Ok(mut weights) = state.trust_weights.lock() {
            weights.set(&address, clamped);
        }
    }

    match queue.resolve(&address) {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "resolved", "address": address})),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "no pending review for this address"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("failed to resolve review: {e}")})),
        ),
    }
}
