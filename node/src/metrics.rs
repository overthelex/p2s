use portable_atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    inner: Arc<MetricsInner>,
}

struct MetricsInner {
    start_time: Instant,

    // Card operations
    pub cards_put_total: AtomicU64,
    pub cards_put_success: AtomicU64,
    pub cards_put_rejected: AtomicU64,
    pub cards_get_total: AtomicU64,
    pub cards_get_found: AtomicU64,
    pub cards_get_not_found: AtomicU64,

    // Latency buckets (microseconds): <1ms, <5ms, <10ms, <50ms, <100ms, <500ms, <1s, >1s
    pub put_latency_buckets: [AtomicU64; 8],
    pub get_latency_buckets: [AtomicU64; 8],
    pub put_latency_sum_us: AtomicU64,
    pub get_latency_sum_us: AtomicU64,

    // DHT
    pub dht_records_stored: AtomicU64,
    pub dht_peers_connected: AtomicU64,
    pub dht_queries_in: AtomicU64,
    pub dht_queries_out: AtomicU64,

    // Network
    pub http_requests_total: AtomicU64,
    pub http_errors_total: AtomicU64,

    // Validation
    pub sig_verify_ok: AtomicU64,
    pub sig_verify_fail: AtomicU64,
}

const BUCKET_BOUNDS_US: [u64; 7] = [1_000, 5_000, 10_000, 50_000, 100_000, 500_000, 1_000_000];

impl Metrics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                start_time: Instant::now(),
                cards_put_total: AtomicU64::new(0),
                cards_put_success: AtomicU64::new(0),
                cards_put_rejected: AtomicU64::new(0),
                cards_get_total: AtomicU64::new(0),
                cards_get_found: AtomicU64::new(0),
                cards_get_not_found: AtomicU64::new(0),
                put_latency_buckets: Default::default(),
                get_latency_buckets: Default::default(),
                put_latency_sum_us: AtomicU64::new(0),
                get_latency_sum_us: AtomicU64::new(0),
                dht_records_stored: AtomicU64::new(0),
                dht_peers_connected: AtomicU64::new(0),
                dht_queries_in: AtomicU64::new(0),
                dht_queries_out: AtomicU64::new(0),
                http_requests_total: AtomicU64::new(0),
                http_errors_total: AtomicU64::new(0),
                sig_verify_ok: AtomicU64::new(0),
                sig_verify_fail: AtomicU64::new(0),
            }),
        }
    }

    pub fn inc_put_total(&self) { self.inner.cards_put_total.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_put_success(&self) { self.inner.cards_put_success.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_put_rejected(&self) { self.inner.cards_put_rejected.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_get_total(&self) { self.inner.cards_get_total.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_get_found(&self) { self.inner.cards_get_found.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_get_not_found(&self) { self.inner.cards_get_not_found.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_http_requests(&self) { self.inner.http_requests_total.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_http_errors(&self) { self.inner.http_errors_total.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_sig_ok(&self) { self.inner.sig_verify_ok.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_sig_fail(&self) { self.inner.sig_verify_fail.fetch_add(1, Ordering::Relaxed); }
    #[allow(dead_code)]
    pub fn inc_dht_queries_in(&self) { self.inner.dht_queries_in.fetch_add(1, Ordering::Relaxed); }
    #[allow(dead_code)]
    pub fn inc_dht_queries_out(&self) { self.inner.dht_queries_out.fetch_add(1, Ordering::Relaxed); }
    #[allow(dead_code)]
    pub fn set_peers(&self, n: u64) { self.inner.dht_peers_connected.store(n, Ordering::Relaxed); }
    #[allow(dead_code)]
    pub fn set_records(&self, n: u64) { self.inner.dht_records_stored.store(n, Ordering::Relaxed); }

    pub fn record_put_latency(&self, us: u64) {
        self.inner.put_latency_sum_us.fetch_add(us, Ordering::Relaxed);
        let idx = BUCKET_BOUNDS_US.iter().position(|&b| us < b).unwrap_or(7);
        self.inner.put_latency_buckets[idx].fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_get_latency(&self, us: u64) {
        self.inner.get_latency_sum_us.fetch_add(us, Ordering::Relaxed);
        let idx = BUCKET_BOUNDS_US.iter().position(|&b| us < b).unwrap_or(7);
        self.inner.get_latency_buckets[idx].fetch_add(1, Ordering::Relaxed);
    }

    pub fn render_prometheus(&self) -> String {
        let m = &self.inner;
        let uptime = m.start_time.elapsed().as_secs();
        let labels = ["le_1ms", "le_5ms", "le_10ms", "le_50ms", "le_100ms", "le_500ms", "le_1s", "gt_1s"];

        let mut out = String::with_capacity(2048);

        out.push_str(&format!("# HELP p2s_uptime_seconds Node uptime\np2s_uptime_seconds {uptime}\n"));
        out.push_str(&format!("p2s_cards_put_total {}\n", m.cards_put_total.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_cards_put_success {}\n", m.cards_put_success.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_cards_put_rejected {}\n", m.cards_put_rejected.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_cards_get_total {}\n", m.cards_get_total.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_cards_get_found {}\n", m.cards_get_found.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_cards_get_not_found {}\n", m.cards_get_not_found.load(Ordering::Relaxed)));

        let put_total = m.cards_put_total.load(Ordering::Relaxed).max(1);
        let put_avg = m.put_latency_sum_us.load(Ordering::Relaxed) / put_total;
        out.push_str(&format!("p2s_put_latency_avg_us {put_avg}\n"));
        for (i, label) in labels.iter().enumerate() {
            out.push_str(&format!("p2s_put_latency_bucket{{bucket=\"{label}\"}} {}\n", m.put_latency_buckets[i].load(Ordering::Relaxed)));
        }

        let get_total = m.cards_get_total.load(Ordering::Relaxed).max(1);
        let get_avg = m.get_latency_sum_us.load(Ordering::Relaxed) / get_total;
        out.push_str(&format!("p2s_get_latency_avg_us {get_avg}\n"));
        for (i, label) in labels.iter().enumerate() {
            out.push_str(&format!("p2s_get_latency_bucket{{bucket=\"{label}\"}} {}\n", m.get_latency_buckets[i].load(Ordering::Relaxed)));
        }

        out.push_str(&format!("p2s_dht_records_stored {}\n", m.dht_records_stored.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_dht_peers_connected {}\n", m.dht_peers_connected.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_dht_queries_in {}\n", m.dht_queries_in.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_dht_queries_out {}\n", m.dht_queries_out.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_http_requests_total {}\n", m.http_requests_total.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_http_errors_total {}\n", m.http_errors_total.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_sig_verify_ok {}\n", m.sig_verify_ok.load(Ordering::Relaxed)));
        out.push_str(&format!("p2s_sig_verify_fail {}\n", m.sig_verify_fail.load(Ordering::Relaxed)));

        out
    }

    pub fn render_json(&self) -> serde_json::Value {
        let m = &self.inner;
        let put_total = m.cards_put_total.load(Ordering::Relaxed).max(1);
        let get_total = m.cards_get_total.load(Ordering::Relaxed).max(1);
        serde_json::json!({
            "uptime_seconds": m.start_time.elapsed().as_secs(),
            "cards": {
                "put_total": m.cards_put_total.load(Ordering::Relaxed),
                "put_success": m.cards_put_success.load(Ordering::Relaxed),
                "put_rejected": m.cards_put_rejected.load(Ordering::Relaxed),
                "get_total": m.cards_get_total.load(Ordering::Relaxed),
                "get_found": m.cards_get_found.load(Ordering::Relaxed),
                "get_not_found": m.cards_get_not_found.load(Ordering::Relaxed),
            },
            "latency_us": {
                "put_avg": m.put_latency_sum_us.load(Ordering::Relaxed) / put_total,
                "get_avg": m.get_latency_sum_us.load(Ordering::Relaxed) / get_total,
            },
            "dht": {
                "records_stored": m.dht_records_stored.load(Ordering::Relaxed),
                "peers_connected": m.dht_peers_connected.load(Ordering::Relaxed),
                "queries_in": m.dht_queries_in.load(Ordering::Relaxed),
                "queries_out": m.dht_queries_out.load(Ordering::Relaxed),
            },
            "http": {
                "requests_total": m.http_requests_total.load(Ordering::Relaxed),
                "errors_total": m.http_errors_total.load(Ordering::Relaxed),
            },
            "signature": {
                "verify_ok": m.sig_verify_ok.load(Ordering::Relaxed),
                "verify_fail": m.sig_verify_fail.load(Ordering::Relaxed),
            },
        })
    }
}
