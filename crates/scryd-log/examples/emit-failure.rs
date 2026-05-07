//! Initializes scryd-log and emits one categorized failure entry, so the
//! AC `cargo run -p scryd-log --example emit-failure 2>&1 | grep -F
//! '"category":"auth rejection"'` can confirm the JSON wire format.

use scryd_log::{category, log_failure};

fn main() {
    // Force JSON output regardless of whether stderr is a TTY in the dev
    // environment. We just install our own subscriber here; the binary
    // crate's serve path will use scryd_log::init().
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_target(true)
        .with_env_filter("debug")
        .with_writer(std::io::stderr)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    log_failure!(
        category = category::AUTH_REJECTION,
        account_id = "primary",
        folder = "INBOX"
    );
}
