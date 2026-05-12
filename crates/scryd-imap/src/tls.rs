//! TLS connect helper. Uses `tokio-rustls`. Default trust anchors
//! come from the `webpki-roots` bundle baked into the binary; an
//! optional per-account `tls_ca_path` lets operators add a private
//! CA's PEM bundle for self-signed corporate IMAP servers without
//! rebuilding scryd. There is no per-account cert-skip-verify
//! option — the spec prohibits it.
//!
//! The returned stream type is wrapped in `tokio_util::compat::Compat`
//! so it presents the `futures::io::{AsyncRead, AsyncWrite}` traits
//! that `async-imap` requires.

use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

use crate::ClientError;

/// futures-traits-compatible TLS stream `async-imap` consumes.
pub type TlsStream = Compat<tokio_rustls::client::TlsStream<TcpStream>>;

/// Connect over TCP and complete a TLS handshake to `host:port` using
/// the default `webpki-roots` trust anchors.
pub async fn connect_tls(host: &str, port: u16) -> Result<TlsStream, ClientError> {
    do_connect(host, port, default_client_config()).await
}

/// Connect over TCP and complete a TLS handshake to `host:port` using
/// the certificates in `ca_pem_path` as the sole trust anchors.
/// `webpki-roots` is NOT mixed in — the operator's PEM bundle is
/// authoritative for this account.
pub async fn connect_tls_with_ca(
    host: &str,
    port: u16,
    ca_pem_path: &Path,
) -> Result<TlsStream, ClientError> {
    let cfg = client_config_from_pem(ca_pem_path)?;
    do_connect(host, port, cfg).await
}

async fn do_connect(host: &str, port: u16, cfg: ClientConfig) -> Result<TlsStream, ClientError> {
    let tcp = TcpStream::connect((host, port))
        .await
        .map_err(|e| ClientError::Connect(e.to_string()))?;

    let connector = TlsConnector::from(Arc::new(cfg));
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

fn client_config_from_pem(ca_pem_path: &Path) -> Result<ClientConfig, ClientError> {
    let pem_bytes = std::fs::read(ca_pem_path).map_err(|e| {
        ClientError::TlsHandshake(format!("read CA PEM at {}: {e}", ca_pem_path.display()))
    })?;
    let mut roots = RootCertStore::empty();
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&pem_bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            ClientError::TlsHandshake(format!(
                "parse PEM at {}: {e}",
                ca_pem_path.display()
            ))
        })?;
    if certs.is_empty() {
        return Err(ClientError::TlsHandshake(format!(
            "no certificates in PEM file {}",
            ca_pem_path.display()
        )));
    }
    for cert in certs {
        roots.add(cert).map_err(|e| {
            ClientError::TlsHandshake(format!(
                "add certificate from {}: {e}",
                ca_pem_path.display()
            ))
        })?;
    }
    Ok(ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth())
}
