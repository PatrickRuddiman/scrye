//! TCP+TLS connect, server greeting, LOGIN. Each failure path emits the
//! corresponding spec failure-category log line so the operator can
//! triage with `journalctl --user -u scryd | jq`.

use scryd_log::{category, log_failure};

use crate::client::Client;
use crate::tls::{connect_tls, TlsStream};
use crate::ClientError;

/// Connect to `host:port`, complete TLS, read the server greeting, and
/// LOGIN. On any failure path, emits the matching `connect failure` /
/// `tls failure` / `auth rejection` log line tagged with the account id.
pub async fn login(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    account_id: &str,
) -> Result<Client<TlsStream>, ClientError> {
    // 1. TCP + TLS handshake.
    let tls = match connect_tls(host, port).await {
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

    // 2. Build the imap client over the TLS stream.
    let client = async_imap::Client::new(tls);

    // 3. LOGIN. async-imap's `login` returns the Session on success or
    //    a tuple on failure that we reduce to AuthRejected.
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
