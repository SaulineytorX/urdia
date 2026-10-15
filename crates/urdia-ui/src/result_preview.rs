//! Temporary, explicit static-result previews on a fresh browser origin.
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use uuid::Uuid;

pub const RESULT_PREVIEW_TTL: Duration = Duration::from_secs(900);

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultPreviewRequest {
    pub request_id: Uuid,
    pub turn_id: Uuid,
    pub confirm_execution: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultPreviewError {
    Unavailable,
    Unconfirmed,
    Busy,
    Stale,
    Conflict,
    Limit,
    HostUnavailable,
    InvalidSource,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ResultPreviewJob {
    pub request_id: Uuid,
    pub turn_id: Uuid,
    pub status: &'static str,
    pub current_task: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub fingerprint: Option<String>,
    pub file_count: Option<usize>,
    pub expires_in_seconds: u64,
}
impl std::fmt::Debug for ResultPreviewJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResultPreviewJob")
            .field("request_id", &self.request_id)
            .field("status", &self.status)
            .field("url", &"<private>")
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResultPreviewSnapshot {
    pub preparing: bool,
    pub latest: Option<ResultPreviewJob>,
}

pub struct ResultPreviewBundle {
    files: BTreeMap<String, Bytes>,
    fingerprint: String,
}
impl ResultPreviewBundle {
    pub fn new(files: BTreeMap<String, Vec<u8>>) -> Result<Self, ResultPreviewError> {
        if !files.contains_key("index.html") || files.len() > 128 {
            return Err(ResultPreviewError::InvalidSource);
        }
        let mut total = 0;
        let mut hash = Sha256::new();
        let mut output = BTreeMap::new();
        for (name, bytes) in files {
            if !static_name(&name) || bytes.len() > 1_048_576 {
                return Err(ResultPreviewError::InvalidSource);
            }
            total += bytes.len();
            if total > 8_388_608 {
                return Err(ResultPreviewError::InvalidSource);
            }
            hash.update(name.as_bytes());
            hash.update([0]);
            hash.update(Sha256::digest(&bytes));
            output.insert(name, Bytes::from(bytes));
        }
        Ok(Self {
            files: output,
            fingerprint: format!("{:x}", hash.finalize()),
        })
    }
}
fn static_name(name: &str) -> bool {
    name.len() <= 512
        && name
            .chars()
            .all(|c| !c.is_control() && !matches!(c, '\\' | '%' | '?' | '#'))
        && name.split('/').count() <= 9
        && name.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && part.len() <= 128
                && !matches!(
                    part,
                    "node_modules" | "vendor" | "venv" | "target" | "__pycache__"
                )
        })
        && matches!(
            name.rsplit('.')
                .next()
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("html" | "css" | "js" | "svg" | "png" | "jpg" | "jpeg" | "ico" | "woff" | "woff2")
        )
}
fn content_type(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}
#[derive(Clone)]
struct PreviewHttp {
    host: String,
    bundle: Arc<ResultPreviewBundle>,
    active: Arc<AtomicBool>,
    expires: Instant,
}
pub struct ResultPreviewLease {
    state: PreviewHttp,
    stop: Option<oneshot::Sender<()>>,
}
impl ResultPreviewLease {
    pub async fn start(bundle: ResultPreviewBundle) -> std::io::Result<Self> {
        Self::start_for(bundle, RESULT_PREVIEW_TTL).await
    }
    async fn start_for(bundle: ResultPreviewBundle, ttl: Duration) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        // Fresh host AND port prevent storage reuse when the OS recycles a port.
        // RFC6761 reserves .localhost names for loopback; no public DNS required.
        let nonce = format!(
            "{}{}",
            Uuid::new_v4().simple(),
            &Uuid::new_v4().simple().to_string()[..16]
        );
        let state = PreviewHttp {
            host: format!("p-{nonce}.localhost:{}", listener.local_addr()?.port()),
            bundle: Arc::new(bundle),
            active: Arc::new(AtomicBool::new(true)),
            expires: Instant::now() + ttl,
        };
        let (send, receive) = oneshot::channel();
        let app = Router::new()
            .fallback(any(serve_static))
            .with_state(state.clone());
        let active = state.active.clone();
        tokio::spawn(async move {
            let stop = async move {
                tokio::select! { _ = tokio::time::sleep(ttl) => {}, _ = receive => {} }
                active.store(false, Ordering::SeqCst);
            };
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(stop)
                .await;
        });
        Ok(Self {
            state,
            stop: Some(send),
        })
    }
    pub fn stop(&mut self) {
        self.state.active.store(false, Ordering::SeqCst);
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
    fn alive(&self) -> bool {
        self.state.active.load(Ordering::SeqCst) && Instant::now() < self.state.expires
    }
    fn decorate(&self, job: &mut ResultPreviewJob) {
        if !self.alive() {
            job.status = "expired";
            job.url = None;
            return;
        }
        job.url = Some(format!("http://{}/", self.state.host));
        job.fingerprint = Some(self.state.bundle.fingerprint.clone());
        job.file_count = Some(self.state.bundle.files.len());
        job.expires_in_seconds = self
            .state
            .expires
            .saturating_duration_since(Instant::now())
            .as_secs();
    }
}
impl Drop for ResultPreviewLease {
    fn drop(&mut self) {
        self.stop();
    }
}

fn decode_path(path: &str) -> Option<String> {
    let mut output = Vec::new();
    let mut bytes = path.as_bytes().iter().copied();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let a = (bytes.next()? as char).to_digit(16)?;
            let b = (bytes.next()? as char).to_digit(16)?;
            output.push((a * 16 + b) as u8);
        } else {
            output.push(b);
        }
    }
    String::from_utf8(output).ok()
}
async fn serve_static(State(state): State<PreviewHttp>, request: Request) -> Response {
    let status = if !state.active.load(Ordering::SeqCst) || Instant::now() >= state.expires {
        Some(StatusCode::GONE)
    } else if request.headers().get("host").and_then(|v| v.to_str().ok())
        != Some(state.host.as_str())
    {
        Some(StatusCode::NOT_FOUND)
    } else if request.method() != Method::GET && request.method() != Method::HEAD {
        Some(StatusCode::METHOD_NOT_ALLOWED)
    } else {
        None
    };
    let mut response = if let Some(status) = status {
        status.into_response()
    } else {
        let path = decode_path(request.uri().path()).unwrap_or_default();
        let name = if path == "/" {
            "index.html"
        } else {
            path.strip_prefix('/').unwrap_or("")
        };
        match state.bundle.files.get(name).filter(|_| static_name(name)) {
            Some(bytes) => {
                let body = if request.method() == Method::HEAD {
                    Body::empty()
                } else {
                    Body::from(bytes.clone())
                };
                let mut response = Response::new(body);
                response.headers_mut().insert(
                    "content-type",
                    content_type(name).parse().expect("fixed MIME"),
                );
                response
            }
            None => StatusCode::NOT_FOUND.into_response(),
        }
    };
    for (key, value) in [
        ("cache-control", "no-store"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
        ("x-dns-prefetch-control", "off"),
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
        (
            "permissions-policy",
            "camera=(), microphone=(), geolocation=(), usb=(), serial=(), browsing-topics=()",
        ),
        (
            "content-security-policy",
            "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data: blob:; font-src 'self'; media-src 'self' blob:; worker-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; sandbox allow-scripts allow-same-origin allow-forms allow-modals allow-downloads",
        ),
    ] {
        response
            .headers_mut()
            .insert(key, value.parse().expect("fixed policy"));
    }
    response
}

#[derive(Default)]
pub(crate) struct ResultPreviewStore {
    pub enabled: bool,
    pub jobs: BTreeMap<Uuid, ResultPreviewJob>,
    pub latest: Option<Uuid>,
    pub lease: Option<(Uuid, ResultPreviewLease)>,
}
impl ResultPreviewStore {
    pub fn preparing(&self) -> bool {
        self.latest
            .and_then(|id| self.jobs.get(&id))
            .is_some_and(|j| matches!(j.status, "queued" | "preparing"))
    }
    pub fn stop(&mut self, id: Uuid) {
        if self
            .lease
            .as_ref()
            .is_some_and(|(current, _)| *current == id)
        {
            self.lease.take();
        }
        if let Some(job) = self.jobs.get_mut(&id) {
            job.status = "stopped";
            job.url = None;
            job.expires_in_seconds = 0;
        }
    }
    pub fn snapshot(&self, latest_turn: Option<Uuid>) -> ResultPreviewSnapshot {
        let latest = self
            .latest
            .and_then(|id| self.jobs.get(&id))
            .cloned()
            .map(|mut job| {
                job.current_task = latest_turn == Some(job.turn_id);
                if let Some((id, lease)) = &self.lease
                    && *id == job.request_id
                {
                    lease.decorate(&mut job);
                }
                job
            });
        ResultPreviewSnapshot {
            preparing: self.preparing(),
            latest,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    fn bundle() -> ResultPreviewBundle {
        ResultPreviewBundle::new(BTreeMap::from([(
            "index.html".into(),
            b"<h1>Private project</h1>".to_vec(),
        )]))
        .unwrap()
    }
    #[tokio::test]
    async fn static_origin_is_capability_checked_bounded_and_stoppable() {
        let mut lease = ResultPreviewLease::start(bundle()).await.unwrap();
        let app = Router::new()
            .fallback(any(serve_static))
            .with_state(lease.state.clone());
        for (host, path, method, expected) in [
            ("wrong", "/", "GET", 404),
            (lease.state.host.as_str(), "/api/conversation", "GET", 404),
            (lease.state.host.as_str(), "/%2e%2e/.env", "GET", 404),
            (lease.state.host.as_str(), "/", "POST", 405),
            (lease.state.host.as_str(), "/", "GET", 200),
            (lease.state.host.as_str(), "/", "HEAD", 200),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .method(method)
                        .header("host", host)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), expected);
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert!(
                !response
                    .headers()
                    .contains_key("access-control-allow-origin")
            );
        }
        assert!(lease.state.host.starts_with("p-") && lease.state.host.contains(".localhost:"));
        let other = ResultPreviewLease::start(bundle()).await.unwrap();
        assert_ne!(lease.state.host, other.state.host);
        lease.stop();
        assert!(!lease.alive());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("host", &lease.state.host)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::GONE);
    }
    #[tokio::test]
    async fn expiry_closes_without_extending_or_exposing_the_url() {
        let lease = ResultPreviewLease::start_for(bundle(), Duration::from_millis(5))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!lease.alive());
        let mut job = ResultPreviewJob {
            request_id: Uuid::new_v4(),
            turn_id: Uuid::new_v4(),
            status: "ready",
            current_task: true,
            url: None,
            fingerprint: None,
            file_count: None,
            expires_in_seconds: 0,
        };
        lease.decorate(&mut job);
        assert_eq!(job.status, "expired");
        assert!(job.url.is_none());
    }
    #[test]
    fn source_names_and_bounds_never_admit_configuration_or_paths() {
        for name in [
            ".env",
            "../index.html",
            "config.json",
            "a/.hidden.js",
            "vendor/x.js",
            "/index.html",
            "a\\b.js",
            "a/%2e.js",
        ] {
            assert!(!static_name(name), "{name}");
        }
        assert!(static_name("assets/Hero.PNG"));
        assert_eq!(content_type("assets/Hero.PNG"), "image/png");
        assert!(
            ResultPreviewBundle::new(BTreeMap::from([
                ("index.html".into(), vec![]),
                ("APP.JS".into(), vec![])
            ]))
            .is_ok()
        );
        assert!(ResultPreviewBundle::new(BTreeMap::new()).is_err());
        assert!(
            ResultPreviewBundle::new(BTreeMap::from([("index.html".into(), vec![0; 1_048_577])]))
                .is_err()
        );
    }
}
