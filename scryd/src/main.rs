//! `scryd` — operator's CLI + daemon entrypoint. Three operator-visible
//! verbs (`add-account`, `reindex`, `search`) plus a hidden `serve`
//! verb the systemd user unit invokes.

mod exit;
mod uds_client;

use clap::{Parser, Subcommand};

use crate::exit::{bail, ExitCode};

#[derive(Parser, Debug)]
#[command(
    name = "scryd",
    about = "Read-only IMAP indexer & search daemon",
    version
)]
struct Cli {
    #[command(subcommand)]
    verb: Verb,
}

#[derive(Subcommand, Debug)]
enum Verb {
    /// Configure an IMAP account (interactive prompt for credential).
    #[command(name = "add-account")]
    AddAccount(AddAccountArgs),

    /// Trigger a full reindex on the running daemon.
    Reindex,

    /// Run a search query against the running daemon.
    Search(SearchArgs),

    /// Daemon entrypoint (invoked by `systemctl --user start scryd`).
    /// Hidden from `--help` so the operator sees only the three verbs.
    #[command(hide = true)]
    Serve,
}

#[derive(clap::Args, Debug)]
struct AddAccountArgs {
    #[arg(long = "account-id")]
    account_id: Option<String>,
    #[arg(long)]
    host: Option<String>,
    #[arg(long, default_value_t = 993)]
    port: u16,
    #[arg(long)]
    user: Option<String>,
    #[arg(long = "password-stdin")]
    password_stdin: bool,
    #[arg(long, value_delimiter = ',')]
    folders: Option<Vec<String>>,
}

#[derive(clap::Args, Debug)]
struct SearchArgs {
    /// The free-form search query.
    query: String,
    #[arg(long)]
    from: Option<String>,
    #[arg(long)]
    since: Option<String>,
    #[arg(long)]
    until: Option<String>,
    #[arg(long)]
    folder: Option<String>,
    #[arg(long)]
    account: Option<String>,
    #[arg(long, default_value_t = 20)]
    limit: usize,
    #[arg(long, value_parser = ["fulltext", "semantic", "hybrid"], default_value = "fulltext")]
    mode: String,
    #[arg(long)]
    json: bool,
}

fn main() {
    let cli = Cli::parse();
    match cli.verb {
        Verb::Serve => run_serve(),
        Verb::Reindex => run_reindex(),
        Verb::Search(_) => run_search_stub(),
        Verb::AddAccount(_) => run_add_account_stub(),
    }
}

fn run_serve() {
    // Tokio runtime + scryd_runtime::serve — task 16's serve() is
    // partially deferred (the live IMAP scheduler integration), so v1
    // exits cleanly with a clear message rather than panicking.
    eprintln!("scryd serve: live integration deferred — start the partial daemon manually for development");
    std::process::exit(ExitCode::Error.into_raw());
}

fn run_reindex() {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => bail(ExitCode::Error, "error", &e.to_string()),
    };
    rt.block_on(async {
        let client = match uds_client::UdsClient::from_env() {
            Ok(c) => c,
            Err(e) => bail(ExitCode::Error, "error", &e.to_string()),
        };
        match client.post("/internal/reindex").await {
            Ok(resp) if resp.status == 202 => {
                println!("reindex started; search continues to serve during rebuild");
                println!("check progress with: journalctl --user -u scryd");
            }
            Ok(resp) if resp.status == 409 => {
                println!("a reindex is already running");
            }
            Ok(resp) => {
                bail(
                    ExitCode::DaemonRejected,
                    "daemon-rejected",
                    &format!("status {}", resp.status),
                );
            }
            Err(e) => match e {
                uds_client::ClientError::DaemonNotRunning(_) => bail(
                    ExitCode::DaemonNotRunning,
                    "daemon-not-running",
                    "scryd is not running for this user. Start it with: systemctl --user start scryd",
                ),
                other => {
                    let code = other.exit_code();
                    bail(code, "error", &other.to_string());
                }
            },
        }
    });
}

fn run_search_stub() {
    // Filled in by task 21.
    eprintln!("scryd search: implementation lands in task 21");
    std::process::exit(ExitCode::Error.into_raw());
}

fn run_add_account_stub() {
    // Filled in by task 22.
    eprintln!("scryd add-account: implementation lands in task 22");
    std::process::exit(ExitCode::Error.into_raw());
}
