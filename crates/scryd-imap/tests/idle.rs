use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use scryd_imap::idle::{emit_idle_drop, emit_idle_enter, emit_idle_recycle, IDLE_RECYCLE};
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
        .with_env_filter("scryd_imap=info,info")
        .finish();
    tracing::subscriber::with_default(subscriber, f);
    cap.snapshot()
}

#[test]
fn emit_idle_enter_logs_kind_idle_enter() {
    let captured = capture(|| {
        emit_idle_enter("primary", "INBOX");
    });
    let last = captured.lines().last().expect("at least one line");
    assert!(last.contains("\"level\":\"INFO\""));
    assert!(last.contains("\"kind\":\"idle_enter\""));
    assert!(last.contains("\"account_id\":\"primary\""));
    assert!(last.contains("\"folder\":\"INBOX\""));
}

#[test]
fn emit_idle_drop_logs_kind_idle_drop_with_reason() {
    let captured = capture(|| {
        emit_idle_drop("primary", "INBOX", "server closed channel");
    });
    let last = captured.lines().last().unwrap();
    assert!(last.contains("\"kind\":\"idle_drop\""));
    assert!(last.contains("\"reason\":\"server closed channel\""));
}

#[test]
fn emit_idle_recycle_logs_kind_idle_recycle() {
    let captured = capture(|| {
        emit_idle_recycle("primary", "INBOX");
    });
    let last = captured.lines().last().unwrap();
    assert!(last.contains("\"kind\":\"idle_recycle\""));
}

#[test]
fn idle_recycle_is_25_minutes() {
    assert_eq!(IDLE_RECYCLE, std::time::Duration::from_secs(25 * 60));
}
