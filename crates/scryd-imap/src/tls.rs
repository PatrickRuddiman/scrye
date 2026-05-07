//! TLS connect helper. Uses `tokio-rustls` with `webpki-roots` trust
//! anchors — no system OpenSSL dependency, no per-account skip-verify
//! option (the spec disallows it).
//!
//! The returned stream type is wrapped in `tokio_util::compat::Compat` so
//! it presents the `futures::io::{AsyncRead, AsyncWrite}` traits that
//! `async-imap` requires.

use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

use crate::ClientError;

/// futures-traits-compatible TLS stream `async-imap` consumes.
pub type TlsStream = Compat<tokio_rustls::client::TlsStream<TcpStream>>;

/// Connect over TCP and complete a TLS handshake to `host:port`. Trust
/// anchors come from the `webpki-roots` bundle baked into the binary;
/// system trust stores are not consulted.
pub async fn connect_tls(host: &str, port: u16) -> Result<TlsStream, ClientError> {
    let tcp = TcpStream::connect((host, port))
        .await
        .map_err(|e| ClientError::Connect(e.to_string()))?;

    let connector = TlsConnector::from(Arc::new(default_client_config()));
    let server_name = ServerName::try_from(host.to_string())
        .map_err(|e| ClientError::TlsHandshake(format!("invalid host: {e}")))?;

    let tls = connector
        .connect(server_name, tcp)
        .await
        .map_err(|e| ClientError::TlsHandshake(e.to_string()))?;

    Ok(tls.compat())
}

fn default_client_config() -> ClientConfig {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}
