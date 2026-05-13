//! Download the XTR GGUF asset bundle to `<target>/`, verify its
//! SHA-256, and extract the three files witchcraft's `t5-quantized`
//! backend loads at runtime (`config.json`, `tokenizer.json`,
//! `xtr.gguf`). Re-running the helper when all three are already
//! present is a no-op.
//!
//! Under v0.3.1's service model the daemon spawns this helper on first
//! start; uid is inherited from the systemd unit account (default: scryd).
//! The bundle URL points at a github release on this repo that is
//! produced by the `witchcraft-assets` workflow against a pinned
//! dropbox/witchcraft revision.

use std::path::PathBuf;
use std::process::Command;

use clap::Parser;
use sha2::Digest;
use tokio::io::AsyncWriteExt;

/// Default URL of the xtr-gguf tarball. The
/// `witchcraft-assets-<short-rev>` release is produced by
/// `.github/workflows/witchcraft-assets.yml`.
const DEFAULT_BUNDLE_URL: &str =
    "https://github.com/PatrickRuddiman/scrye/releases/download/witchcraft-assets-1370cd5/xtr-gguf-1370cd5.tar.gz";

/// Pinned SHA-256 of the expected tarball. Resolution order:
///   1. `--sha256 <hex>` flag.
///   2. `SCRYD_PINNED_WEIGHTS_SHA256` env var.
///   3. The build-time `WEIGHTS_SHA256` env var (set by the release
///      pipeline if it ever wants to pin a different rev without
///      editing the source).
///   4. The compile-time default below — the SHA-256 of the published
///      `xtr-gguf-1370cd5.tar.gz` asset.
const DEFAULT_BUNDLE_SHA256: &str = match option_env!("WEIGHTS_SHA256") {
    Some(s) => s,
    None => "60fbdd2e4289542bac1c2c61ef63e064ed583df427f2d328ad90274547b7f177",
};

/// Files witchcraft's `t5-quantized` backend loads from the assets
/// path. The tarball contains a single top-level directory with these
/// three files; we extract with `--strip-components=1` so they land at
/// `<target>/<filename>` regardless of the inner dirname.
const REQUIRED_FILES: &[&str] = &[
    "config.json",
    "tokenizer.json",
    "xtr.gguf",
];

const TMP_TARBALL_NAME: &str = ".fetch-tmp.tar.gz";

#[derive(Parser, Debug)]
#[command(
    name = "scryd-fetch-weights",
    about = "Download and extract the XTR INT4 asset bundle for scryd",
    version
)]
struct Args {
    /// Where to extract the bundle's contents. Defaults to
    /// `$XDG_DATA_HOME/scryd/assets/`, falling back to
    /// `$HOME/.local/share/scryd/assets/`.
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

    let target_dir = match args.target {
        Some(p) => p,
        None => default_target_dir()?,
    };
    let url = args.url.unwrap_or_else(|| DEFAULT_BUNDLE_URL.to_string());
    let expected_sha = args
        .sha256
        .or_else(|| std::env::var("SCRYD_PINNED_WEIGHTS_SHA256").ok())
        .unwrap_or_else(|| DEFAULT_BUNDLE_SHA256.to_string());

    if all_required_present(&target_dir) {
        println!("assets ok (already present)");
        return Ok(());
    }

    tokio::fs::create_dir_all(&target_dir).await?;
    tighten_dir_perms(&target_dir).await?;

    let tmp_tarball = target_dir.join(TMP_TARBALL_NAME);
    download_to(&url, &tmp_tarball).await?;

    let actual = sha256_hex(&tmp_tarball).await?;
    if actual != expected_sha {
        let _ = tokio::fs::remove_file(&tmp_tarball).await;
        anyhow::bail!(
            "downloaded bundle hash {actual} != expected {expected_sha}; aborting"
        );
    }

    extract_tarball(&tmp_tarball, &target_dir)?;
    let _ = tokio::fs::remove_file(&tmp_tarball).await;

    let missing: Vec<&str> = REQUIRED_FILES
        .iter()
        .copied()
        .filter(|f| !target_dir.join(f).exists())
        .collect();
    if !missing.is_empty() {
        anyhow::bail!("bundle extracted but missing required files: {missing:?}");
    }

    let bytes: u64 = REQUIRED_FILES
        .iter()
        .filter_map(|f| std::fs::metadata(target_dir.join(f)).ok().map(|m| m.len()))
        .sum();
    println!("assets ok ({bytes} bytes across {} files)", REQUIRED_FILES.len());
    Ok(())
}

fn all_required_present(target_dir: &std::path::Path) -> bool {
    REQUIRED_FILES.iter().all(|f| target_dir.join(f).exists())
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
        .user_agent("scryd-fetch-weights/0.3.1")
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

fn extract_tarball(tarball: &std::path::Path, target_dir: &std::path::Path) -> anyhow::Result<()> {
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(tarball)
        .arg("-C")
        .arg(target_dir)
        .arg("--strip-components=1")
        .status()?;
    if !status.success() {
        anyhow::bail!("tar -xzf exited {status}");
    }
    Ok(())
}

#[cfg(unix)]
async fn write_file_secure(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
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
    tokio::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).await?;
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
