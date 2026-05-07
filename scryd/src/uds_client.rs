//! Minimal HTTP/1.1-over-UDS client. Tasks 21–23 use this to talk to
//! the running daemon's api.

#![allow(dead_code)] // get / at_path / socket_path / body field are tasks 21–23 surface

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::exit::ExitCode;

const READ_BUDGET: Duration = Duration::from_secs(30);
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
    /// Resolve `$XDG_RUNTIME_DIR/scryd/scryd.sock`. Returns
    /// [`ExitCode::Error`]-shaped error if XDG_RUNTIME_DIR is unset.
    pub fn from_env() -> Result<Self, ClientError> {
        let runtime = std::env::var("XDG_RUNTIME_DIR").map_err(|_| {
            ClientError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "XDG_RUNTIME_DIR is not set",
            ))
        })?;
        if runtime.is_empty() {
            return Err(ClientError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "XDG_RUNTIME_DIR is empty",
            )));
        }
        let socket_path = PathBuf::from(runtime).join("scryd").join("scryd.sock");
        Ok(Self { socket_path })
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
        self.request("GET", path).await
    }

    pub async fn post(&self, path: &str) -> Result<HttpResponse, ClientError> {
        self.request("POST", path).await
    }

    async fn request(&self, method: &str, path: &str) -> Result<HttpResponse, ClientError> {
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
        let read = tokio::time::timeout(READ_BUDGET, async {
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
