//! Plain-TCP (no TLS) IMAP connect helper. **Test-only path.** The
//! production path is always TLS via [`crate::tls::connect_tls`]; this
//! sibling exists so integration tests can speak to local test
//! fixtures (GreenMail) that don't ship a real certificate.
//!
//! The toggle is per-account via `AccountCfg.tls = false`; it defaults
//! to `true`. There is no per-account cert-skip-verify option — that
//! prohibition stands.

use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

use crate::ClientError;

/// futures-traits-compatible plain TCP stream `async-imap` consumes.
pub type PlainStream = Compat<TcpStream>;

/// Connect over plain TCP (no TLS) to `host:port`. Errors map to
/// [`ClientError::Connect`].
pub async fn connect_plain(host: &str, port: u16) -> Result<PlainStream, ClientError> {
    let tcp = TcpStream::connect((host, port))
        .await
        .map_err(|e| ClientError::Connect(e.to_string()))?;
    Ok(tcp.compat())
}
