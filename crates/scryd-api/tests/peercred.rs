#![cfg(unix)]
//! Peercred unit tests. As of v0.3.1, peercred enforcement is OPT-IN
//! via `[server] require_peer_uid = true`; the daemon defaults to an
//! open socket. These tests still exercise the enforcement primitives
//! (`check_peer_uid` + `extract_peer_uid`) because they stay the
//! implementation when the operator turns it on.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use scryd_api::{bind, check_peer_uid, extract_peer_uid, ApiError};
use tempfile::TempDir;
use tokio::net::UnixStream;
use tracing_subscriber::fmt::MakeWriter;

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

#[test]
fn check_peer_uid_accepts_matching_uid() {
    assert!(check_peer_uid(1000, 1000, 4711).is_ok());
}

#[test]
fn check_peer_uid_rejects_mismatched_uid_with_log() {
    let captured = capture(|| {
        let result = check_peer_uid(2000, 1000, 4711);
        assert!(matches!(
            result,
            Err(ApiError::NonOwner { peer_uid: 2000 })
        ));
    });
    let line = captured.lines().last().expect("at least one log line");
    assert!(line.contains("\"category\":\"non-owner-user connection rejection\""));
    assert!(line.contains("\"peer_uid\":2000"));
    assert!(line.contains("\"expected_uid\":1000"));
    assert!(line.contains("\"peer_pid\":4711"));
    assert!(line.contains("\"level\":\"ERROR\""));
}

#[tokio::test]
async fn extract_peer_uid_returns_current_process_uid_for_self_connection() {
    let dir = TempDir::new().unwrap();
    let scryd_dir = dir.path().join("scryd");
    let listener = bind(&scryd_dir, 0o666).await.unwrap();
    let sock = scryd_dir.join("scryd.sock");

    let connect = tokio::spawn(async move { UnixStream::connect(&sock).await.unwrap() });
    let (server_side, _addr) = listener.accept().await.unwrap();
    let _client_side = connect.await.unwrap();

    let peer_uid = extract_peer_uid(&server_side).unwrap();
    assert_eq!(peer_uid, nix::unistd::getuid().as_raw());
}
