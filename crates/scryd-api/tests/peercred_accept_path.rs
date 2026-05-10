#![cfg(unix)]
//! Integration test that exercises the full `check_stream_peer` accept
//! path against a real Unix domain socket plus the `check_peer_uid`
//! reject path with the new `expected_uid` log field.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use scryd_api::{bind, check_peer_uid, init_peercred, ApiError};
use scryd_api::peercred::check_stream_peer;
use serial_test::serial;
use tempfile::TempDir;
use tokio::net::UnixStream;
use tracing_subscriber::fmt::MakeWriter;

const ENV_ALLOWED_UID: &str = "SCRYD_ALLOWED_UID";

#[derive(Clone, Default)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl CaptureWriter {
    fn snapshot(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}
impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl<'a> MakeWriter<'a> for CaptureWriter {
    type Writer = CaptureWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn capture<F: FnOnce()>(f: F) -> String {
    let cap = CaptureWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_target(true)
        .with_writer(cap.clone())
        .with_env_filter("scryd_api=warn,warn")
        .finish();
    tracing::subscriber::with_default(subscriber, f);
    cap.snapshot()
}

#[tokio::test]
#[serial]
async fn check_stream_peer_accepts_self_when_env_uid_matches_getuid() {
    let my_uid = nix::unistd::getuid().as_raw();
    std::env::set_var(ENV_ALLOWED_UID, my_uid.to_string());
    init_peercred().expect("init_peercred should succeed for valid uid env var");

    let dir = TempDir::new().unwrap();
    let scryd_dir = dir.path().join("scryd");
    let listener = bind(&scryd_dir, 0o666).await.unwrap();
    let sock = scryd_dir.join("scryd.sock");

    let connect = tokio::spawn(async move { UnixStream::connect(&sock).await.unwrap() });
    let (server_side, _addr) = listener.accept().await.unwrap();
    let _client_side = connect.await.unwrap();

    let peer_uid =
        check_stream_peer(&server_side, true).expect("self-connection must pass peercred");
    assert_eq!(peer_uid, my_uid);

    std::env::remove_var(ENV_ALLOWED_UID);
}

#[tokio::test]
#[serial]
async fn check_stream_peer_returns_ok_when_disabled_even_for_mismatched_uid() {
    // v0.3.1 default: require_peer_uid = false. The disabled path must
    // return the peer uid without comparison or log-line emission.
    let mismatch_uid = nix::unistd::getuid().as_raw().wrapping_add(1);
    std::env::set_var(ENV_ALLOWED_UID, mismatch_uid.to_string());

    let dir = TempDir::new().unwrap();
    let scryd_dir = dir.path().join("scryd");
    let listener = bind(&scryd_dir, 0o666).await.unwrap();
    let sock = scryd_dir.join("scryd.sock");

    let connect = tokio::spawn(async move { UnixStream::connect(&sock).await.unwrap() });
    let (server_side, _addr) = listener.accept().await.unwrap();
    let _client_side = connect.await.unwrap();

    let captured = capture(|| {
        let res = check_stream_peer(&server_side, false);
        assert!(res.is_ok(), "disabled peercred must accept any peer");
    });
    assert!(
        !captured.contains("non-owner-user connection rejection"),
        "no rejection log expected when disabled, got: {captured}"
    );

    std::env::remove_var(ENV_ALLOWED_UID);
}

#[test]
#[serial]
fn check_peer_uid_rejection_log_carries_expected_uid_from_env() {
    let mismatch_uid = nix::unistd::getuid().as_raw().wrapping_add(1);
    std::env::set_var(ENV_ALLOWED_UID, mismatch_uid.to_string());
    let observed_peer_uid = nix::unistd::getuid().as_raw();

    let captured = capture(|| {
        let result = check_peer_uid(observed_peer_uid, mismatch_uid, 1234);
        assert!(matches!(result, Err(ApiError::NonOwner { .. })));
    });

    std::env::remove_var(ENV_ALLOWED_UID);

    let rejection_lines: Vec<&str> = captured
        .lines()
        .filter(|l| l.contains("non-owner-user connection rejection"))
        .collect();
    assert_eq!(
        rejection_lines.len(),
        1,
        "expected exactly one rejection log line, got: {captured}"
    );
    let line = rejection_lines[0];
    assert!(line.contains(&format!("\"expected_uid\":{mismatch_uid}")));
    assert!(line.contains(&format!("\"peer_uid\":{observed_peer_uid}")));
    assert!(line.contains("\"peer_pid\":1234"));
}
