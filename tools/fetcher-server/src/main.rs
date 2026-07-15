//! Deterministic local server for SPEC-8 "Fetcher".
//! Endpoints: /health, /search?q=, /download (streamed ~8s, logs ABORT on
//! client disconnect), /flaky (process-global 500,500,200 cycle; run probes
//! serially for deterministic attribution).

use axum::{
    body::Body,
    extract::{ConnectInfo, Query},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static FLAKY_COUNTER: AtomicU64 = AtomicU64::new(0);
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

const ADJECTIVES: [&str; 16] = [
    "amber", "brisk", "coral", "dusty", "eager", "fuzzy", "glossy", "hazel",
    "icy", "jolly", "keen", "lunar", "mossy", "noble", "opal", "prime",
];
const NOUNS: [&str; 16] = [
    "anchor", "beacon", "cobalt", "delta", "ember", "falcon", "garnet",
    "harbor", "island", "jasper", "kernel", "lagoon", "marble", "nectar",
    "orbit", "prism",
];

fn hash(s: &str) -> u64 {
    // FNV-1a: deterministic across runs/platforms.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn request_id() -> u64 {
    REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn tagged(mut response: Response, id: u64) -> Response {
    response.headers_mut().insert(
        "x-fetcher-request-id",
        HeaderValue::from_str(&id.to_string()).expect("numeric request id is a valid header"),
    );
    response
}

async fn health(ConnectInfo(peer): ConnectInfo<SocketAddr>) -> Response {
    let id = request_id();
    eprintln!(
        "ts_ms={} request_id={} peer={} HEALTH",
        timestamp_ms(), id, peer
    );
    tagged("ok".into_response(), id)
}

async fn search(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    struct SearchGuard {
        id: u64,
        peer: SocketAddr,
        completed: bool,
    }
    impl Drop for SearchGuard {
        fn drop(&mut self) {
            if !self.completed {
                eprintln!(
                    "ts_ms={} request_id={} peer={} SEARCH_CANCEL",
                    timestamp_ms(), self.id, self.peer
                );
            }
        }
    }

    let id = request_id();
    let q = params.get("q").cloned().unwrap_or_default();
    let q_lc = q.to_lowercase();
    // Deterministic artificial latency: 150-300 ms based on the query text.
    let delay = 150 + (hash(&q_lc) % 151);
    eprintln!(
        "ts_ms={} request_id={} peer={} SEARCH_START q={q:?} delay={delay}ms",
        timestamp_ms(), id, peer
    );
    let mut guard = SearchGuard { id, peer, completed: false };
    tokio::time::sleep(Duration::from_millis(delay)).await;

    // Deterministic corpus: 512 names; return up to 20 substring matches.
    let mut results = Vec::new();
    for i in 0..512u64 {
        let name = format!(
            "{}-{}-{:04}",
            ADJECTIVES[(i % 16) as usize],
            NOUNS[((i / 16) % 16) as usize],
            (hash(&i.to_string()) % 10000)
        );
        if q_lc.is_empty() || name.contains(&q_lc) {
            results.push(serde_json::json!({
                "id": i,
                "name": name,
                "score": (hash(&format!("{q_lc}{i}")) % 1000) as f64 / 10.0,
            }));
            if results.len() >= 20 {
                break;
            }
        }
    }
    eprintln!(
        "ts_ms={} request_id={} peer={} SEARCH_DONE q={q:?} delay={delay}ms results={}",
        timestamp_ms(), id, peer, results.len()
    );
    guard.completed = true;
    tagged(axum::Json(results).into_response(), id)
}

async fn download(ConnectInfo(peer): ConnectInfo<SocketAddr>) -> Response {
    // 8 MiB in 64 chunks of 128 KiB, one every 125 ms => ~8 s total.
    const CHUNK: usize = 128 * 1024;
    const CHUNKS: usize = 64;
    let id = request_id();
    eprintln!(
        "ts_ms={} request_id={} peer={} DOWNLOAD_START",
        timestamp_ms(), id, peer
    );
    let stream = async_stream(CHUNK, CHUNKS, id, peer);
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, (CHUNK * CHUNKS).to_string())
        .body(Body::from_stream(stream))
        .unwrap();
    tagged(response, id)
}

fn async_stream(
    chunk: usize,
    chunks: usize,
    request_id: u64,
    peer: SocketAddr,
) -> impl futures_core_stream::Stream<Item = Result<Vec<u8>, std::io::Error>> {
    // Tiny hand-rolled stream to avoid extra deps; ABORT is logged when the
    // receiver is dropped before completion (client disconnected).
    struct Guard {
        sent: usize,
        total: usize,
        request_id: u64,
        peer: SocketAddr,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            if self.sent < self.total {
                eprintln!(
                    "ts_ms={} request_id={} peer={} ABORT /download after {}/{} chunks",
                    timestamp_ms(), self.request_id, self.peer, self.sent, self.total
                );
            } else {
                eprintln!(
                    "ts_ms={} request_id={} peer={} DOWNLOAD_COMPLETE chunks={}",
                    timestamp_ms(), self.request_id, self.peer, self.total
                );
            }
        }
    }
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(2);
    tokio::spawn(async move {
        let mut guard = Guard { sent: 0, total: chunks, request_id, peer };
        let payload = vec![0xABu8; chunk];
        for _ in 0..chunks {
            tokio::time::sleep(Duration::from_millis(125)).await;
            if tx.send(Ok(payload.clone())).await.is_err() {
                return; // receiver gone -> Guard logs ABORT
            }
            guard.sent += 1;
        }
    });
    tokio_stream_wrapper::ReceiverStream::new(rx)
}

// Minimal local aliases so we don't pull tokio-stream/futures as deps of note.
mod futures_core_stream {
    pub use futures_core::Stream;
}
mod tokio_stream_wrapper {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    pub struct ReceiverStream<T> {
        rx: tokio::sync::mpsc::Receiver<T>,
    }
    impl<T> ReceiverStream<T> {
        pub fn new(rx: tokio::sync::mpsc::Receiver<T>) -> Self {
            Self { rx }
        }
    }
    impl<T> futures_core::Stream for ReceiverStream<T> {
        type Item = T;
        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
            self.rx.poll_recv(cx)
        }
    }
}

async fn flaky(ConnectInfo(peer): ConnectInfo<SocketAddr>) -> Response {
    let id = request_id();
    let n = FLAKY_COUNTER.fetch_add(1, Ordering::SeqCst);
    if n % 3 == 2 {
        eprintln!(
            "ts_ms={} request_id={} peer={} FLAKY attempt={} -> 200",
            timestamp_ms(), id, peer, n + 1
        );
        tagged(axum::Json(serde_json::json!({ "attempt": n + 1 })).into_response(), id)
    } else {
        eprintln!(
            "ts_ms={} request_id={} peer={} FLAKY attempt={} -> 500",
            timestamp_ms(), id, peer, n + 1
        );
        tagged(
            (StatusCode::INTERNAL_SERVER_ERROR, "synthetic failure").into_response(),
            id,
        )
    }
}

async fn reset_flaky(ConnectInfo(peer): ConnectInfo<SocketAddr>) -> Response {
    let id = request_id();
    FLAKY_COUNTER.store(0, Ordering::SeqCst);
    eprintln!(
        "ts_ms={} request_id={} peer={} FLAKY_RESET",
        timestamp_ms(), id, peer
    );
    tagged("ok".into_response(), id)
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("FETCHER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7878);
    let app = Router::new()
        .route("/health", get(health))
        .route("/search", get(search))
        .route("/download", get(download))
        .route("/flaky/reset", get(reset_flaky))
        .route("/flaky", get(flaky));
    let addr = format!("127.0.0.1:{port}");
    eprintln!("fetcher-server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}
