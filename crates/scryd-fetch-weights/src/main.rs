//! Download the XTR T5 weights bundle to `<target>/xtr-weights.gguf`,
//! verify SHA-256, and exit. Re-running the helper against an
//! already-correct file is a no-op. Refuses to run as root — the
//! per-user install model in build-and-packaging slice §3 Decision 7
//! prohibits a root-owned weights file.

use std::path::PathBuf;

use clap::Parser;
use sha2::Digest;
use tokio::io::AsyncWriteExt;

/// Hugging Face URL for the GGUF-quantized XTR weights bundle. The
/// install script (`ops/install.sh`) calls this binary at first run.
const DEFAULT_WEIGHTS_URL: &str =
    "https://huggingface.co/dropbox/witchcraft-weights/resolve/main/xtr-weights.gguf";

/// Pinned SHA-256 of the expected weights bundle. Set to a placeholder
/// — the build-and-packaging slice's first concrete release picks the
/// canonical hash from the Hugging Face revision and bakes it in. v1
/// allows callers to override via `--sha256` so the test harness and
/// air-gapped installers don't need a network round-trip.
const DEFAULT_WEIGHTS_SHA256: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

const FILENAME: &str = "xtr-weights.gguf";

#[derive(Parser, Debug)]
#[command(
    name = "scryd-fetch-weights",
    about = "Download and verify the XTR T5 weights bundle for scryd",
    version
)]
struct Args {
    /// Where to write `xtr-weights.gguf`. Defaults to
    /// `$XDG_DATA_HOME/scryd/assets/`, then `$HOME/.local/share/scryd/assets/`.
    #[arg(long)]
    target: Option<PathBuf>,
    /// Override the download URL (operators with no internet, tests).
    #[arg(long)]
    url: Option<String>,
    /// Override the expected SHA-256 hex (lowercased).
    #[arg(long)]
    sha256: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    refuse_as_root()?;

    let target_dir = match args.target {
        Some(p) => p,
        None => default_target_dir()?,
    };
    let target = target_dir.join(FILENAME);
    let url = args.url.unwrap_or_else(|| DEFAULT_WEIGHTS_URL.to_string());
    let expected_sha = args.sha256.unwrap_or_else(|| DEFAULT_WEIGHTS_SHA256.to_string());

    if target.exists() {
        match sha256_hex(&target).await {
            Ok(actual) if actual == expected_sha => {
                println!("weights ok");
                return Ok(());
            }
            Ok(actual) => {
                eprintln!(
                    "scryd-fetch-weights: existing file hash {actual} != expected {expected_sha}; re-downloading"
                );
            }
            Err(e) => {
                eprintln!(
                    "scryd-fetch-weights: hashing existing file failed: {e}; re-downloading"
                );
            }
        }
    }

    tokio::fs::create_dir_all(&target_dir).await?;
    tighten_dir_perms(&target_dir).await?;

    let tmp = with_tmp_suffix(&target);
    download_to(&url, &tmp).await?;

    let actual = sha256_hex(&tmp).await?;
    if actual != expected_sha {
        let _ = tokio::fs::remove_file(&tmp).await;
        anyhow::bail!(
            "downloaded weights hash {actual} != expected {expected_sha}; aborting"
        );
    }
    tokio::fs::rename(&tmp, &target).await?;

    let bytes = tokio::fs::metadata(&target).await?.len();
    println!("weights ok ({bytes} bytes)");
    Ok(())
}

#[cfg(unix)]
fn refuse_as_root() -> anyhow::Result<()> {
    // SAFETY: getuid() is always safe to call.
    let uid = unsafe { libc_getuid() };
    if uid == 0 && std::env::var("SCRYD_FETCH_WEIGHTS_ALLOW_ROOT").is_err() {
        anyhow::bail!(
            "scryd-fetch-weights must run as a non-root user (set SCRYD_FETCH_WEIGHTS_ALLOW_ROOT only in tests)"
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn refuse_as_root() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
unsafe extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}

fn default_target_dir() -> anyhow::Result<PathBuf> {
    if let Ok(s) = std::env::var("XDG_DATA_HOME") {
        if !s.is_empty() {
            return Ok(PathBuf::from(s).join("scryd").join("assets"));
        }
    }
    let home = std::env::var("HOME")
        .map_err(|_| anyhow::anyhow!("XDG_DATA_HOME and HOME both unset"))?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("scryd")
        .join("assets"))
}

async fn download_to(url: &str, dest: &std::path::Path) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .user_agent("scryd-fetch-weights/0.1")
        .build()?;
    let response = client.get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("download {url} returned status {status}");
    }
    let bytes = response.bytes().await?;
    write_file_secure(dest, &bytes).await?;
    Ok(())
}

#[cfg(unix)]
async fn write_file_secure(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    // tokio::fs::OpenOptions::mode() on unix maps to OpenOptionsExt;
    // calling it on tokio's builder doesn't require the trait import.
    let mut opts = tokio::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true).mode(0o600);
    let mut f = opts.open(path).await?;
    f.write_all(bytes).await?;
    f.sync_all().await?;
    Ok(())
}

#[cfg(not(unix))]
async fn write_file_secure(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut f = tokio::fs::File::create(path).await?;
    f.write_all(bytes).await?;
    f.sync_all().await?;
    Ok(())
}

#[cfg(unix)]
async fn tighten_dir_perms(dir: &std::path::Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn tighten_dir_perms(_dir: &std::path::Path) -> anyhow::Result<()> {
    Ok(())
}

async fn sha256_hex(path: &std::path::Path) -> anyhow::Result<String> {
    let bytes = tokio::fs::read(path).await?;
    let digest = sha2::Sha256::digest(&bytes);
    Ok(hex::encode(digest))
}

fn with_tmp_suffix(p: &std::path::Path) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}
