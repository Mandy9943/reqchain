use crate::model::Method;
use crate::request::{EffectiveBody, EffectiveRequest};
use std::time::{Duration, Instant};

/// How long to wait for a TCP/TLS connection before giving up. A host that is
/// down or firewalled off usually manifests as a connect that never completes.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Ceiling on a whole request/response round trip. Without one, a server that
/// accepts the connection and then goes silent hangs the caller forever — and
/// in the desktop app the executor mutex is held for the whole round trip, so
/// that one request would also freeze saving and every watcher-driven reload,
/// with no cancel and no message. 30 s is long enough for a slow gateway and
/// short enough that the UI recovers on its own.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("cannot read file `{path}`: {message}")]
    File { path: String, message: String },
}

#[derive(Debug, Clone)]
pub struct AuthStep {
    pub endpoint_id: String,
    pub request: Option<EffectiveRequest>,
    pub status: u16,
    pub body: String,
    pub from_cache: bool,
}

#[derive(Debug)]
pub struct RunResult {
    pub status: u16,
    pub elapsed_ms: u128,
    pub size_bytes: usize,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub effective: EffectiveRequest,
    pub auth_trace: Vec<AuthStep>,
}

impl RunResult {
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

#[derive(Clone)]
pub struct Runner {
    client: reqwest::Client,
}

impl Default for Runner {
    fn default() -> Self {
        Runner::new()
    }
}

impl Runner {
    pub fn new() -> Runner {
        Runner::with_timeouts(CONNECT_TIMEOUT, REQUEST_TIMEOUT)
    }

    /// A runner with explicit timeouts. [`Runner::new`] is this with
    /// [`CONNECT_TIMEOUT`] and [`REQUEST_TIMEOUT`]; tests use short ones to
    /// exercise the timeout path without waiting half a minute.
    pub fn with_timeouts(connect: Duration, total: Duration) -> Runner {
        Runner {
            client: reqwest::Client::builder()
                .connect_timeout(connect)
                .timeout(total)
                .build()
                .expect("client builds"),
        }
    }

    pub async fn send(&self, req: &EffectiveRequest) -> Result<RunResult, ExecError> {
        let method = match req.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
            Method::Head => reqwest::Method::HEAD,
            Method::Options => reqwest::Method::OPTIONS,
        };
        let mut rb = self.client.request(method, &req.url);
        for (k, v) in &req.headers {
            rb = rb.header(k, v)
        }
        rb = match &req.body {
            None => rb,
            Some(EffectiveBody::Text {
                content_type,
                content,
            }) => rb
                .header("content-type", content_type)
                .body(content.clone()),
            Some(EffectiveBody::Form { fields }) => rb.form(&fields.clone()),
            Some(EffectiveBody::Multipart { fields, files }) => {
                let mut form = reqwest::multipart::Form::new();
                for (k, v) in fields {
                    form = form.text(k.clone(), v.clone())
                }
                for (k, path) in files {
                    let bytes = std::fs::read(path).map_err(|e| ExecError::File {
                        path: path.clone(),
                        message: e.to_string(),
                    })?;
                    form = form.part(
                        k.clone(),
                        reqwest::multipart::Part::bytes(bytes).file_name(path.clone()),
                    );
                }
                rb.multipart(form)
            }
            Some(EffectiveBody::Binary { path }) => {
                let bytes = std::fs::read(path).map_err(|e| ExecError::File {
                    path: path.clone(),
                    message: e.to_string(),
                })?;
                rb.body(bytes)
            }
        };

        let started = Instant::now();
        let res = rb
            .send()
            .await
            .map_err(|e| ExecError::Transport(e.to_string()))?;
        let status = res.status().as_u16();
        let headers = res
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<binary>").to_string()))
            .collect();
        let body = res
            .bytes()
            .await
            .map_err(|e| ExecError::Transport(e.to_string()))?
            .to_vec();
        Ok(RunResult {
            status,
            elapsed_ms: started.elapsed().as_millis(),
            size_bytes: body.len(),
            headers,
            body,
            effective: req.clone(),
            auth_trace: Vec::new(),
        })
    }
}
