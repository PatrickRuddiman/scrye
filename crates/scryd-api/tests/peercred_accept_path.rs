#![cfg(unix)]
//! Integration test that exercises the full `check_stream_peer` accept
//! path against a real Unix domain socket. v0.3.1 default is
//! `require_peer_uid = false` (open socket); when `true`, only the
//! daemon's own uid is accepted.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use scryd_api::bind;
use scryd_api::peercred::check_stream_peer;
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

#[tokio::test]
async fn check_stream_peer_accepts_self_when_enabled() {
    let my_uid = nix::unistd::getuid().as_raw();
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
}

#[tokio::test]
async fn check_stream_peer_disabled_emits_no_rejection_log() {
    // v0.3.1 default: require_peer_uid = false. The disabled path must
    // return the peer uid without comparison or log-line emission.
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
}
