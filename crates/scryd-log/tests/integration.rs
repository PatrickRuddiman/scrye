//! Integration tests for `scryd-log`.
//!
//! Each emission test installs its own scoped JSON subscriber that writes into
//! a captured `Vec<u8>`, asserts the line, and unwinds — so tests don't fight
//! over the global subscriber.
//!
//! `init_is_idempotent` is the only test that touches the process-global
//! subscriber, since `scryd_log::init()` is itself a global side effect.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use scryd_log::{category, kind, log_failure, log_lifecycle, log_request, InitError};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    fn snapshot(&self) -> String {
        let buf = self.0.lock().unwrap();
        String::from_utf8(buf.clone()).expect("utf-8 capture")
    }
}

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Install a JSON subscriber that captures one line, run `f`, then return the
/// captured payload as a parsed `serde_json::Value`. The subscriber is scoped
/// to this thread so other tests' globals stay untouched.
fn capture<F: FnOnce()>(f: F) -> serde_json::Value {
    let cap = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_target(true)
        .with_writer(cap.clone())
        .with_env_filter("debug")
        .finish();
    tracing::subscriber::with_default(subscriber, f);

    let raw = cap.snapshot();
    let line = raw.lines().last().expect("at least one log line");
    serde_json::from_str(line).expect("valid json line")
}

#[test]
fn log_failure_emits_error_with_category() {
    let event = capture(|| {
        log_failure!(
            category = category::AUTH_REJECTION,
            account_id = "primary"
        );
    });

    assert_eq!(event["level"], "ERROR");
    assert_eq!(event["category"], "auth rejection");
    assert_eq!(event["account_id"], "primary");
}

#[test]
fn log_failure_severity_warn_downgrades_level() {
    let event = capture(|| {
        log_failure!(
            severity = warn,
            category = category::SINGLE_MESSAGE_PARSE_FAILURE,
            account_id = "primary",
            message_id = "primary:abc@x"
        );
    });

    assert_eq!(event["level"], "WARN");
    assert_eq!(event["category"], "single-message parse failure");
    assert_eq!(event["message_id"], "primary:abc@x");
}

#[test]
fn log_lifecycle_emits_info_with_kind() {
    let event = capture(|| {
        log_lifecycle!(kind = kind::STARTUP, version = "0.1.0", accounts = 2);
    });

    assert_eq!(event["level"], "INFO");
    assert_eq!(event["kind"], "startup");
    assert_eq!(event["version"], "0.1.0");
    assert_eq!(event["accounts"], 2);
}

#[test]
fn log_request_always_debug_with_request_kind() {
    let event = capture(|| {
        log_request!(method = "GET", path = "/search", status = 200, duration_ms = 17);
    });

    assert_eq!(event["level"], "DEBUG");
    assert_eq!(event["kind"], "request");
    assert_eq!(event["status"], 200);
}

#[test]
fn init_is_idempotent() {
    // First call may either succeed (we are first) or fail with
    // SubscriberAlreadySet (some other test installed a global). Both
    // outcomes count as "first"; what we need is that the SECOND call
    // always returns AlreadyInitialized.
    let _ = scryd_log::init();
    match scryd_log::init() {
        Err(InitError::AlreadyInitialized) => {}
        other => panic!("expected AlreadyInitialized on second init, got {other:?}"),
    }
}

#[test]
fn closed_set_categories_are_grep_stable() {
    // Verifies the literal string values; if these change, downstream
    // grep-based AC checks (and journal-side filtering) break.
    assert_eq!(category::CONNECT_FAILURE, "connect failure");
    assert_eq!(category::TLS_FAILURE, "tls failure");
    assert_eq!(category::AUTH_REJECTION, "auth rejection");
    assert_eq!(category::PUSH_CHANNEL_DROP, "push-channel drop");
    assert_eq!(
        category::UID_VALIDITY_RESET,
        "uid-validity reset triggering re-sync"
    );
    assert_eq!(
        category::SINGLE_MESSAGE_PARSE_FAILURE,
        "single-message parse failure"
    );
    assert_eq!(
        category::SINGLE_MESSAGE_FT_INDEXER_FAILURE,
        "single-message full-text indexer failure"
    );
    assert_eq!(
        category::SINGLE_MESSAGE_SEMANTIC_INDEXER_FAILURE,
        "single-message semantic indexer failure"
    );
    assert_eq!(category::DISK_FULL, "disk-full");
    assert_eq!(category::CONFIG_PARSE_ERROR, "configuration parse error");
    assert_eq!(
        category::CONFIG_PERMISSION_ERROR,
        "configuration permission error"
    );
    assert_eq!(
        category::NON_OWNER_REJECTION,
        "non-owner-user connection rejection"
    );
}

#[test]
fn closed_set_kinds_are_grep_stable() {
    assert_eq!(kind::STARTUP, "startup");
    assert_eq!(kind::SHUTDOWN, "shutdown");
    assert_eq!(kind::SYNC_PASS_START, "sync_pass_start");
    assert_eq!(kind::SYNC_PASS_COMPLETE, "sync_pass_complete");
    assert_eq!(kind::BACKFILL_PROGRESS, "backfill_progress");
    assert_eq!(kind::IDLE_ENTER, "idle_enter");
    assert_eq!(kind::IDLE_DROP, "idle_drop");
    assert_eq!(kind::IDLE_RECYCLE, "idle_recycle");
    assert_eq!(kind::UIDVALIDITY_RESET, "uidvalidity_reset");
    assert_eq!(kind::REINDEX_START, "reindex_start");
    assert_eq!(kind::REINDEX_COMPLETE, "reindex_complete");
    assert_eq!(kind::ACCOUNT_RECONCILED, "account_reconciled");
    assert_eq!(kind::REQUEST, "request");
}
