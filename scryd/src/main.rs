//! `scryd` — operator's CLI + daemon entrypoint. Three operator-visible
//! verbs (`add-account`, `reindex`, `search`) plus a hidden `serve`
//! verb the systemd user unit invokes.

mod config_writer;
mod exit;
mod output;
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

    /// Rotate the IMAP credential for a configured account.
    #[command(name = "rotate-password", about = "Rotate the IMAP credential for a configured account")]
    RotatePassword(RotatePasswordArgs),

    /// Remove a configured IMAP account.
    #[command(name = "remove-account", about = "Remove a configured IMAP account")]
    RemoveAccount(RemoveAccountArgs),

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
struct RotatePasswordArgs {
    account_id: String,
    #[arg(long = "password-stdin")]
    password_stdin: bool,
}

#[derive(clap::Args, Debug)]
struct RemoveAccountArgs {
    account_id: String,
    #[arg(long)]
    yes: bool,
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
    /// Multi-value account scope. `--accounts foo,bar` returns hits
    /// only from those accounts. Empty / absent = all accounts. The
    /// consumer's higher-layer api drives this from per-end-user
    /// policy; scryd does not enforce it itself.
    #[arg(long, value_delimiter = ',')]
    accounts: Option<Vec<String>>,
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
        Verb::Search(args) => run_search(args),
        Verb::AddAccount(args) => run_add_account(args),
        Verb::RotatePassword(args) => run_rotate_password(args),
        Verb::RemoveAccount(args) => run_remove_account(args),
    }
}

fn run_serve() {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => bail(ExitCode::Error, "error", &e.to_string()),
    };
    match rt.block_on(scryd_runtime::serve()) {
        Ok(()) => std::process::exit(0),
        Err(e) => bail(ExitCode::Error, "error", &e.to_string()),
    }
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
            Err(uds_client::ClientError::DaemonNotRunning(_)) => bail(
                ExitCode::DaemonNotRunning,
                "daemon-not-running",
                "scryd is not running for this user. Start it with: sudo systemctl start scryd",
            ),
            Err(e) => bail(e.exit_code(), "error", &e.to_string()),
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

fn run_search(args: SearchArgs) {
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
            Err(uds_client::ClientError::DaemonNotRunning(_)) => bail(
                ExitCode::DaemonNotRunning,
                "daemon-not-running",
                "scryd is not running for this user. Start it with: sudo systemctl start scryd",
            ),
            Err(e) => bail(e.exit_code(), "error", &e.to_string()),
        };

        let url = build_search_url(&args);
        let resp = match client.get(&url).await {
            Ok(r) => r,
            Err(uds_client::ClientError::DaemonNotRunning(_)) => bail(
                ExitCode::DaemonNotRunning,
                "daemon-not-running",
                "scryd is not running for this user. Start it with: systemctl --user start scryd",
            ),
            Err(e) => bail(e.exit_code(), "error", &e.to_string()),
        };

        if resp.status >= 400 {
            let msg = std::str::from_utf8(&resp.body).unwrap_or("(non-utf8 body)");
            bail(
                ExitCode::DaemonRejected,
                "daemon-rejected",
                &format!("status {}: {}", resp.status, msg.trim()),
            );
        }

        if args.json {
            // Emit body verbatim (assumed valid utf-8 JSON).
            let body = String::from_utf8_lossy(&resp.body);
            print!("{body}");
            return;
        }

        let parsed: serde_json::Value = match serde_json::from_slice(&resp.body) {
            Ok(v) => v,
            Err(e) => bail(ExitCode::Error, "error", &format!("parse response: {e}")),
        };
        let tty = output::stdout_is_tty();
        let rendered = output::render_search(&parsed, tty);
        print!("{rendered}");
    });
}

fn build_search_url(args: &SearchArgs) -> String {
    let mut url = String::from("/search?q=");
    url.push_str(&urlencoding::encode(&args.query));
    if let Some(v) = &args.from {
        url.push_str("&from=");
        url.push_str(&urlencoding::encode(v));
    }
    if let Some(v) = &args.since {
        url.push_str("&since=");
        url.push_str(&urlencoding::encode(v));
    }
    if let Some(v) = &args.until {
        url.push_str("&until=");
        url.push_str(&urlencoding::encode(v));
    }
    if let Some(v) = &args.folder {
        url.push_str("&folder=");
        url.push_str(&urlencoding::encode(v));
    }
    if let Some(v) = &args.account {
        url.push_str("&account=");
        url.push_str(&urlencoding::encode(v));
    }
    if let Some(ids) = &args.accounts {
        let joined = ids.join(",");
        if !joined.is_empty() {
            url.push_str("&account_ids=");
            url.push_str(&urlencoding::encode(&joined));
        }
    }
    url.push_str(&format!("&limit={}", args.limit));
    url.push_str(&format!("&mode={}", args.mode));
    url
}

fn run_add_account(args: AddAccountArgs) {
    use std::io::{BufRead, Read, Write};
    use std::path::PathBuf;

    let id_re = regex::Regex::new(r"^[a-z0-9_-]+$").expect("valid regex");

    // Resolve fields from flags or interactive prompts.
    let id = match args.account_id {
        Some(s) => s,
        None => prompt_required("account id"),
    };
    if !id_re.is_match(&id) {
        bail(
            ExitCode::BadInput,
            "bad-input",
            &format!("account id `{id}` must match ^[a-z0-9_-]+$"),
        );
    }

    let host = args.host.unwrap_or_else(|| prompt_required("imap host"));
    let port = args.port;
    if port == 0 {
        bail(ExitCode::BadInput, "bad-input", "port must be 1..=65535");
    }
    let user = args.user.unwrap_or_else(|| prompt_required("username"));

    let password = if args.password_stdin {
        let mut buf = String::new();
        std::io::stdin().lock().read_to_string(&mut buf).unwrap_or(0);
        buf.trim_end_matches(['\n', '\r']).to_string()
    } else {
        rpassword::prompt_password("app password: ").unwrap_or_default()
    };
    if password.is_empty() {
        bail(ExitCode::BadInput, "bad-input", "password cannot be empty");
    }

    let folders = args.folders.unwrap_or_else(|| {
        let raw = prompt_with_default("folders [INBOX]", "INBOX");
        raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    });
    if folders.is_empty() {
        bail(ExitCode::BadInput, "bad-input", "at least one folder is required");
    }

    let entry = config_writer::AccountEntry {
        id: id.clone(),
        host,
        port,
        user,
        password,
        folders,
    };

    let config_path = match resolve_config_path() {
        Ok(p) => p,
        Err(msg) => bail(ExitCode::ConfigError, "config-error", &msg),
    };

    if let Err(e) = config_writer::upsert_account(&config_path, &entry) {
        bail(e.exit_code(), "config-error", &e.to_string());
    }

    chown_to_scryd(&config_path);

    let display_path = pretty_home(&config_path);
    println!("account '{id}' saved to {display_path}");

    print_restart_hint();

    let _ = std::path::PathBuf::new(); // silence unused-import for tests
    let _: PathBuf = config_path;
    let _: &mut dyn Write = &mut std::io::stdout();
    let _: &mut dyn BufRead = &mut std::io::BufReader::new(std::io::empty());
}

fn run_rotate_password(args: RotatePasswordArgs) {
    use std::io::Read;

    let id_re = regex::Regex::new(r"^[a-z0-9_-]+$").expect("valid regex");
    if !id_re.is_match(&args.account_id) {
        bail(
            ExitCode::BadInput,
            "bad-input",
            &format!("account id `{}` must match ^[a-z0-9_-]+$", args.account_id),
        );
    }

    let password = if args.password_stdin {
        let mut buf = String::new();
        std::io::stdin().lock().read_to_string(&mut buf).unwrap_or(0);
        buf.trim_end_matches(['\n', '\r']).to_string()
    } else {
        rpassword::prompt_password("new app password: ").unwrap_or_default()
    };
    if password.is_empty() {
        bail(ExitCode::BadInput, "bad-input", "password cannot be empty");
    }

    let config_path = match resolve_config_path() {
        Ok(p) => p,
        Err(msg) => bail(ExitCode::ConfigError, "config-error", &msg),
    };

    if let Err(e) = config_writer::rotate_password(&config_path, &args.account_id, &password) {
        bail(e.exit_code(), "config-error", &e.to_string());
    }

    chown_to_scryd(&config_path);
    println!("password rotated for '{}'", args.account_id);
    print_restart_hint();
}

fn run_remove_account(args: RemoveAccountArgs) {
    use std::io::{BufRead, Write};

    let id_re = regex::Regex::new(r"^[a-z0-9_-]+$").expect("valid regex");
    if !id_re.is_match(&args.account_id) {
        bail(
            ExitCode::BadInput,
            "bad-input",
            &format!("account id `{}` must match ^[a-z0-9_-]+$", args.account_id),
        );
    }

    let config_path = match resolve_config_path() {
        Ok(p) => p,
        Err(msg) => bail(ExitCode::ConfigError, "config-error", &msg),
    };

    if !args.yes {
        eprint!(
            "remove account '{}' from {}? [y/N]: ",
            args.account_id,
            config_path.display()
        );
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        let trimmed = line.trim();
        if !matches!(trimmed, "y" | "Y" | "yes") {
            println!("aborted");
            return;
        }
    }

    if let Err(e) = config_writer::remove_account(&config_path, &args.account_id) {
        bail(e.exit_code(), "config-error", &e.to_string());
    }

    chown_to_scryd(&config_path);
    println!("account '{}' removed", args.account_id);
    print_restart_hint();
}

/// Effective uid for the elevation check. Honors `SCRYD_CLI_FAKE_EUID`
/// when set so integration tests can exercise the elevation gate
/// without actually running as root. The env var is test-only and
/// should never be set in production.
fn cli_effective_euid() -> u32 {
    if let Ok(s) = std::env::var("SCRYD_CLI_FAKE_EUID") {
        if let Ok(v) = s.parse::<u32>() {
            return v;
        }
    }
    nix::unistd::geteuid().as_raw()
}

/// Chown the freshly-written config to `scryd:scryd` so the daemon
/// (running under that uid) can read it. Best-effort: silently
/// no-ops when the `scryd` system user does not exist (test envs)
/// or when the caller's euid is not root (chown would fail anyway).
fn chown_to_scryd(path: &std::path::Path) {
    if cli_effective_euid() != 0 {
        return;
    }
    if let Ok(Some(user)) = nix::unistd::User::from_name("scryd") {
        let _ = nix::unistd::chown(path, Some(user.uid), Some(user.gid));
    }
}

/// Probe the daemon's UDS and print the appropriate hint:
///   - daemon reachable → `apply changes: sudo systemctl restart scryd`
///   - daemon not running → `apply changes: sudo systemctl start scryd`
fn print_restart_hint() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let reachable = rt.block_on(async {
        let client = match uds_client::UdsClient::from_env() {
            Ok(c) => c,
            Err(_) => return false,
        };
        match client.get("/accounts").await {
            Ok(_) => true,
            Err(uds_client::ClientError::DaemonNotRunning(_)) => false,
            Err(_) => true,
        }
    });
    if reachable {
        println!("apply changes: sudo systemctl restart scryd");
    } else {
        println!("apply changes: sudo systemctl start scryd");
    }
}

fn prompt_required(label: &str) -> String {
    use std::io::{BufRead, Write as _};
    print!("{label}: ");
    let _ = std::io::stdout().flush();
    let stdin = std::io::stdin();
    let mut line = String::new();
    let _ = stdin.lock().read_line(&mut line);
    let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
    if trimmed.is_empty() {
        bail(
            ExitCode::BadInput,
            "bad-input",
            &format!("{label} cannot be empty"),
        );
    }
    trimmed
}

fn prompt_with_default(label: &str, default: &str) -> String {
    use std::io::{BufRead, Write as _};
    print!("{label}: ");
    let _ = std::io::stdout().flush();
    let stdin = std::io::stdin();
    let mut line = String::new();
    let _ = stdin.lock().read_line(&mut line);
    let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed
    }
}

fn resolve_config_path() -> Result<std::path::PathBuf, String> {
    let euid = nix::unistd::geteuid().as_raw();
    scryd::path_resolution::resolve_config_path_for(euid, &scryd::path_resolution::SystemEnv)
}

fn pretty_home(p: &std::path::Path) -> String {
    if let Ok(home) = std::env::var("HOME") {
        let s = p.display().to_string();
        if let Some(rest) = s.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    p.display().to_string()
}
