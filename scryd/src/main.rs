//! `scryd` — the single-purpose mailbox daemon.
//!
//! It does exactly three things, with no other control surface or client:
//!   1. fetches IMAP mail for the account whose login matches `USER_EMAIL`,
//!   2. indexes every message with witchcraft (semantic + full-text), and
//!   3. serves search over MCP on a loopback TCP port.
//!
//! Configuration is the on-disk TOML file; the MCP server is the only way in.
//! There are no subcommands and no UDS/CLI client — `systemctl --user start
//! scryd` (or a bare `scryd`) just runs the daemon.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "scryd",
    about = "IMAP indexer + MCP search daemon (single mailbox, scoped to USER_EMAIL)",
    long_about = "Fetches IMAP mail for the USER_EMAIL account, indexes it with witchcraft, \
                  and serves search over MCP on a loopback port. Configure via the TOML \
                  config file; the MCP server is the only client interface.",
    version
)]
struct Cli {}

fn main() {
    // Workspace feature unification leaves rustls with both `aws-lc-rs` (via
    // scryd-imap / tokio-rustls defaults) and `ring` (via scryd-fetch-weights →
    // reqwest's rustls-tls path) compiled in. rustls 0.23 refuses to
    // auto-select when both are present and panics on the first
    // `ClientConfig::builder()` call. Pick aws-lc-rs explicitly at process
    // startup so every TLS code path inherits a known provider.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("install rustls aws-lc-rs crypto provider");

    // Cap rayon's global thread pool to mirror the tokio worker cap below.
    // candle (via witchcraft) uses rayon for its embed pass; the default pool
    // size is available_parallelism(), which pegs all cores during heavy
    // backfill and starves the IMAP supervisor + MCP server. Install before any
    // candle code runs — the global pool is set once and never resized.
    // `build_global` returns Err if already installed; ignore it.
    let rayon_workers = workers();
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(rayon_workers)
        .build_global();

    // Parse args purely to honor `--help` / `--version`; there are no
    // subcommands. Unknown arguments are rejected by clap.
    let _ = Cli::parse();

    run_daemon();
}

/// Leave 2 cores for the rest of the system. tokio's default is num_cpus,
/// which is too greedy for a background indexer that also competes with the
/// witchcraft embedding work on the blocking pool. `available_parallelism`
/// reads cgroup limits, so a daemon under `CPUQuota=200%` sees 2, not the host
/// count.
fn workers() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .saturating_sub(2)
        .max(1)
}

fn run_daemon() -> ! {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers())
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => fail(&e.to_string()),
    };
    match rt.block_on(scryd_runtime::serve()) {
        Ok(()) => std::process::exit(0),
        Err(e) => fail(&e.to_string()),
    }
}

/// Print a `scryd: error: <message>` line to stderr and exit non-zero.
fn fail(message: &str) -> ! {
    eprintln!("scryd: error: {message}");
    std::process::exit(1);
}
