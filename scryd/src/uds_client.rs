//! Minimal HTTP/1.1-over-UDS client. Tasks 21–23 use this to talk to
//! the running daemon's api.

#![allow(dead_code)] // get / at_path / socket_path / body field are tasks 21–23 surface

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::exit::ExitCode;

/// Read budget for fast admin/status requests (connect-and-drain < 1 s
/// normally, 30 s is already very generous).
const READ_BUDGET: Duration = Duration::from_secs(30);

/// Read budget for `scryd search`. Under an active indexing backlog
/// witchcraft linearly scans all un-clustered embeddings before the
/// index cascade runs, producing 80–130 s observed latencies on a
/// 1,200-row queue (Standard_B4ms, 4 vCPU). 180 s gives ~50 % headroom
/// above the 126 s pressure-test maximum while keeping the CLI from
/// falsely declaring failure when the daemon is working correctly.
///
/// Status, accounts, message-raw, and all admin endpoints remain on the
/// short [`READ_BUDGET`] so they stay responsive during backlog.
const SEARCH_READ_BUDGET: Duration = Duration::from_secs(180);

const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("scryd is not running for this user (no socket at {0})")]
    DaemonNotRunning(PathBuf),
    #[error("connect to {0}: {1}")]
    Connect(PathBuf, #[source] std::io::Error),
    #[error("io while talking to daemon: {0}")]
    Io(#[from] std::io::Error),
    #[error("daemon returned non-2xx: {status} {message}")]
    DaemonRejected { status: u16, message: String },
    #[error("daemon internal error: {0}")]
    DaemonError(String),
    #[error("parse daemon response: {0}")]
    ParseResponse(String),
}

impl ClientError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::DaemonNotRunning(_) => ExitCode::DaemonNotRunning,
            Self::DaemonRejected { status, .. } if *status >= 500 => ExitCode::DaemonRejected,
            Self::DaemonRejected { .. } => ExitCode::DaemonRejected,
            Self::DaemonError(_) => ExitCode::DaemonRejected,
            _ => ExitCode::Error,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub struct UdsClient {
    socket_path: PathBuf,
}

impl UdsClient {
    /// Resolve the socket path: prefer `$XDG_RUNTIME_DIR/scryd/scryd.sock`
    /// (v0.1.0 per-user path); fall back to `/run/scryd/scryd.sock`
    /// (v0.2.0 system path) when the runtime-dir socket doesn't exist.
    /// Returns [`ClientError::DaemonNotRunning`] when neither path exists.
    pub fn from_env() -> Result<Self, ClientError> {
        use scryd::path_resolution::{resolve_socket_path_for, SystemEnv, SYSTEM_SOCKET_PATH};
        let fallback = std::path::Path::new(SYSTEM_SOCKET_PATH);
        match resolve_socket_path_for(&SystemEnv, fallback) {
            Ok(socket_path) => Ok(Self { socket_path }),
            Err(missing) => Err(ClientError::DaemonNotRunning(missing)),
        }
    }

    pub fn at_path(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub async fn get(&self, path: &str) -> Result<HttpResponse, ClientError> {
        self.request("GET", path, READ_BUDGET).await
    }

    /// Issue a GET with the extended read budget for `/search` endpoints.
    ///
    /// `witchcraft::search` holds the state mutex while linearly scanning
    /// un-clustered embeddings; under a reindex backlog this legitimately
    /// takes 80–130 s even though the daemon is making correct progress.
    /// Callers that care about responsiveness (status, accounts, admin)
    /// must use [`get`] instead so they stay fast.
    pub async fn get_search(&self, path: &str) -> Result<HttpResponse, ClientError> {
        self.request("GET", path, SEARCH_READ_BUDGET).await
    }

    pub async fn post(&self, path: &str) -> Result<HttpResponse, ClientError> {
        self.request("POST", path, READ_BUDGET).await
    }

    async fn request(
        &self,
        method: &str,
        path: &str,
        budget: Duration,
    ) -> Result<HttpResponse, ClientError> {
        if !self.socket_path.exists() {
            return Err(ClientError::DaemonNotRunning(self.socket_path.clone()));
        }
        let mut stream = match UnixStream::connect(&self.socket_path).await {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused =>
            {
                return Err(ClientError::DaemonNotRunning(self.socket_path.clone()));
            }
            Err(e) => return Err(ClientError::Connect(self.socket_path.clone(), e)),
        };

        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).await?;
        stream.flush().await?;

        let mut buf = Vec::new();
        let read = tokio::time::timeout(budget, async {
            let mut chunk = [0u8; 8192];
            loop {
                let n = stream.read(&mut chunk).await?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > MAX_RESPONSE_BYTES {
                    return Err(std::io::Error::other("response exceeds read budget"));
                }
            }
            Ok::<_, std::io::Error>(())
        })
        .await
        .map_err(|_| ClientError::Io(std::io::Error::other("daemon read timeout")))??;
        let _ = read;

        parse_http_response(&buf)
    }
}

fn parse_http_response(raw: &[u8]) -> Result<HttpResponse, ClientError> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| ClientError::ParseResponse("no header/body separator".into()))?;
    let header_block = &raw[..split];
    let body = raw[split + 4..].to_vec();

    let header_str =
        std::str::from_utf8(header_block).map_err(|e| ClientError::ParseResponse(e.to_string()))?;
    let status_line = header_str
        .split("\r\n")
        .next()
        .ok_or_else(|| ClientError::ParseResponse("empty status line".into()))?;
    let mut parts = status_line.split_whitespace();
    let _http_version = parts.next();
    let status_code: u16 = parts
        .next()
        .ok_or_else(|| ClientError::ParseResponse("status code missing".into()))?
        .parse()
        .map_err(|e: std::num::ParseIntError| ClientError::ParseResponse(e.to_string()))?;

    Ok(HttpResponse {
        status: status_code,
        body,
    })
}
