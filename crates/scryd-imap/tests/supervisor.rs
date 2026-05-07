use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use scryd_imap::{
    log_auth_rejection, log_connect_failure, log_push_channel_drop, log_tls_failure,
};
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
        .with_env_filter("scryd_imap=warn,warn")
        .finish();
    tracing::subscriber::with_default(subscriber, f);
    cap.snapshot()
}

fn last_line(s: &str) -> &str {
    s.lines().last().expect("at least one log line")
}

#[test]
fn log_connect_failure_emits_correct_category() {
    let captured = capture(|| log_connect_failure("primary", "ECONNREFUSED"));
    let line = last_line(&captured);
    assert!(line.contains("\"level\":\"ERROR\""));
    assert!(line.contains("\"category\":\"connect failure\""));
    assert!(line.contains("\"account_id\":\"primary\""));
    assert!(line.contains("\"error\":\"ECONNREFUSED\""));
}

#[test]
fn log_tls_failure_emits_correct_category() {
    let captured = capture(|| log_tls_failure("primary", "bad certificate"));
    let line = last_line(&captured);
    assert!(line.contains("\"category\":\"tls failure\""));
    assert!(line.contains("\"error\":\"bad certificate\""));
}

#[test]
fn log_auth_rejection_emits_correct_category() {
    let captured = capture(|| log_auth_rejection("primary", "AUTHENTICATIONFAILED"));
    let line = last_line(&captured);
    assert!(line.contains("\"category\":\"auth rejection\""));
}

#[test]
fn log_push_channel_drop_emits_warn_severity() {
    let captured = capture(|| log_push_channel_drop("primary", "INBOX", "server hangup"));
    let line = last_line(&captured);
    assert!(line.contains("\"level\":\"WARN\""));
    assert!(line.contains("\"category\":\"push-channel drop\""));
    assert!(line.contains("\"folder\":\"INBOX\""));
    assert!(line.contains("\"reason\":\"server hangup\""));
}
