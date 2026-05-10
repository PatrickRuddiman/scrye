//! TCP+TLS connect, server greeting, LOGIN. Each failure path emits the
//! corresponding spec failure-category log line so the operator can
//! triage with `journalctl --user -u scryd | jq`.

use std::path::Path;

use scryd_log::{category, log_failure};

use crate::client::Client;
use crate::plain::{connect_plain, PlainStream};
use crate::tls::{connect_tls, connect_tls_with_ca, TlsStream};
use crate::ClientError;

/// Either a TLS-wrapped (production) or plain-TCP (test fixture) IMAP
/// session. The supervisor matches on this enum to drive the same
/// state-machine over both stream types.
pub enum LoggedIn {
    Tls(Client<TlsStream>),
    Plain(Client<PlainStream>),
}

/// Connect over TLS, complete the IMAP greeting, and LOGIN. Production
/// path. `ca_path` selects the trust anchor: `None` uses the baked-in
/// `webpki-roots`; `Some(p)` uses the PEM bundle at `p` as the sole
/// trust anchor (corporate/self-signed CA support). On any failure
/// path, emits the matching `connect failure` / `tls failure` / `auth
/// rejection` log line tagged with the account id.
pub async fn login_tls(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    account_id: &str,
    ca_path: Option<&Path>,
) -> Result<Client<TlsStream>, ClientError> {
    let tls_result = match ca_path {
        Some(p) => connect_tls_with_ca(host, port, p).await,
        None => connect_tls(host, port).await,
    };
    let tls = match tls_result {
        Ok(s) => s,
        Err(ClientError::Connect(msg)) => {
            log_failure!(
                category = category::CONNECT_FAILURE,
                account_id = %account_id,
                error = %msg
            );
            return Err(ClientError::Connect(msg));
        }
        Err(ClientError::TlsHandshake(msg)) => {
            log_failure!(
                category = category::TLS_FAILURE,
                account_id = %account_id,
                error = %msg
            );
            return Err(ClientError::TlsHandshake(msg));
        }
        Err(other) => return Err(other),
    };

    let client = async_imap::Client::new(tls);
    let session = match client.login(user, password).await {
        Ok(s) => s,
        Err((err, _client)) => {
            let msg = err.to_string();
            log_failure!(
                category = category::AUTH_REJECTION,
                account_id = %account_id,
                error = %msg
            );
            return Err(ClientError::AuthRejected);
        }
    };

    Ok(Client::from_session(session))
}

/// Connect over plain TCP and LOGIN. Test-fixture path. Mirrors the
/// failure-log shape of [`login_tls`] except no `tls failure` branch
/// is reachable.
pub async fn login_plain(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    account_id: &str,
) -> Result<Client<PlainStream>, ClientError> {
    let stream = match connect_plain(host, port).await {
        Ok(s) => s,
        Err(ClientError::Connect(msg)) => {
            log_failure!(
                category = category::CONNECT_FAILURE,
                account_id = %account_id,
                error = %msg
            );
            return Err(ClientError::Connect(msg));
        }
        Err(other) => return Err(other),
    };

    let client = async_imap::Client::new(stream);
    let session = match client.login(user, password).await {
        Ok(s) => s,
        Err((err, _client)) => {
            let msg = err.to_string();
            log_failure!(
                category = category::AUTH_REJECTION,
                account_id = %account_id,
                error = %msg
            );
            return Err(ClientError::AuthRejected);
        }
    };

    Ok(Client::from_session(session))
}

/// Dispatch to TLS or plain login based on `tls`. `ca_path` is
/// honoured only on the TLS path (plain has no trust anchor).
pub async fn login(
    host: &str,
    port: u16,
    tls: bool,
    user: &str,
    password: &str,
    account_id: &str,
    ca_path: Option<&Path>,
) -> Result<LoggedIn, ClientError> {
    if tls {
        login_tls(host, port, user, password, account_id, ca_path)
            .await
            .map(LoggedIn::Tls)
    } else {
        login_plain(host, port, user, password, account_id)
            .await
            .map(LoggedIn::Plain)
    }
}
