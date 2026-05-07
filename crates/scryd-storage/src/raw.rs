//! Raw `.eml` file store.
//!
//! Layout: `<data_dir>/raw/<account_id>/<YYYY>/<MM>/<sha256-hex(message_id)>.eml`.
//! Writes go to `<final>.tmp` mode `0600`, are `fsync`'d, and renamed atomically
//! over the final path. Concurrent writes for the same id complete safely; the
//! resulting file always matches one of the inputs in full, never a partial mix.

use std::path::{Path, PathBuf};

use sha2::Digest;
use tokio::io::AsyncWriteExt;

use crate::db::StorageError;

/// Resolve the on-disk path for a message's raw bytes. Deterministic;
/// the same inputs always yield the same path.
pub fn raw_path(
    data_dir: &Path,
    account_id: &str,
    message_id: &str,
    date_unix: i64,
) -> PathBuf {
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(date_unix, 0)
        .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap());
    let yyyy = dt.format("%Y").to_string();
    let mm = dt.format("%m").to_string();

    let hash = sha2::Sha256::digest(message_id.as_bytes());
    let filename = format!("{}.eml", hex::encode(hash));

    data_dir
        .join("raw")
        .join(account_id)
        .join(yyyy)
        .join(mm)
        .join(filename)
}

/// Write raw bytes for a message, atomically. Returns the resolved final path.
pub async fn write_raw(
    data_dir: &Path,
    account_id: &str,
    message_id: &str,
    date_unix: i64,
    bytes: &[u8],
) -> Result<PathBuf, StorageError> {
    let final_path = raw_path(data_dir, account_id, message_id, date_unix);
    if let Some(parent) = final_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
        tighten_dir_perms(parent)?;
    }

    let tmp = with_unique_tmp_suffix(&final_path);
    write_tmp_with_secure_perms(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, &final_path).await?;
    Ok(final_path)
}

/// Open a raw `.eml` for streaming via tokio. The api slice's
/// `/message/:id/raw` endpoint wraps the returned handle in a `ReaderStream`.
pub async fn open_raw(path: &Path) -> Result<tokio::fs::File, StorageError> {
    Ok(tokio::fs::File::open(path).await?)
}

/// Best-effort delete; succeeds even if the file is already gone.
/// v1 never calls this — operator-driven cleanup is out of scope.
#[doc(hidden)]
pub async fn delete_raw(path: &Path) -> Result<(), StorageError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(StorageError::Io(e)),
    }
}

fn with_unique_tmp_suffix(p: &Path) -> PathBuf {
    // Per-call nonce so concurrent writers for the same id never share a
    // sidecar — each one renames its own tmp atomically over the final
    // path; the last rename wins and the file always reflects one
    // complete writer's bytes.
    let mut s = p.as_os_str().to_os_string();
    s.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    PathBuf::from(s)
}

#[cfg(unix)]
fn tighten_dir_perms(parent: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    // The deepest dir gets 0700; ancestors retain whatever the umask
    // produced — they exist on the public filesystem and don't carry secrets.
    let perms = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(parent, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn tighten_dir_perms(_parent: &Path) -> Result<(), StorageError> {
    Ok(())
}

#[cfg(unix)]
async fn write_tmp_with_secure_perms(tmp: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    // tokio::fs::OpenOptions::mode() on unix maps directly to the
    // std::os::unix::fs::OpenOptionsExt method; calling it on tokio's
    // builder does not require the trait import.
    let mut opts = tokio::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true).mode(0o600);
    let mut f = opts.open(tmp).await?;
    f.write_all(bytes).await?;
    f.sync_all().await?;
    Ok(())
}

#[cfg(not(unix))]
async fn write_tmp_with_secure_perms(tmp: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let mut f = tokio::fs::File::create(tmp).await?;
    f.write_all(bytes).await?;
    f.sync_all().await?;
    Ok(())
}
