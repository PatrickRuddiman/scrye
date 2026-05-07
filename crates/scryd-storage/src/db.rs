use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::migrations;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("open meta.sqlite at {0}: {1}")]
    Open(std::path::PathBuf, #[source] rusqlite::Error),
    #[error("permission denied opening meta.sqlite at {0}")]
    Permission(std::path::PathBuf),
    #[error("set pragma: {0}")]
    Pragma(#[source] rusqlite::Error),
    #[error("migration failed: {0}")]
    Migration(#[from] migrations::MigrationError),
}

/// Open `meta.sqlite` for read-write, set WAL/sync/foreign-key pragmas, and
/// run any pending forward migrations. Idempotent across processes; only one
/// writer should hold the resulting connection at a time.
pub fn open(path: &Path) -> Result<Connection, StorageError> {
    let mut conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| classify_open_error(path, e))?;

    set_runtime_pragmas(&conn)?;
    migrations::run(&mut conn)?;
    Ok(conn)
}

/// Open `meta.sqlite` read-only for the api slice's read pool.
pub fn open_read_only(path: &Path) -> Result<Connection, StorageError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| classify_open_error(path, e))?;

    // foreign_keys is the only pragma that's meaningful (and harmless) on a
    // read-only connection. WAL was set when the writer opened.
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(StorageError::Pragma)?;
    Ok(conn)
}

fn set_runtime_pragmas(conn: &Connection) -> Result<(), StorageError> {
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(StorageError::Pragma)?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(StorageError::Pragma)?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(StorageError::Pragma)?;
    Ok(())
}

fn classify_open_error(path: &Path, err: rusqlite::Error) -> StorageError {
    if let rusqlite::Error::SqliteFailure(ffi_err, _) = &err {
        // SQLITE_CANTOPEN with the underlying OS errno tagged as EACCES is the
        // typical "permission denied" surface. Fall through to Open otherwise.
        if ffi_err.code == rusqlite::ErrorCode::CannotOpen
            && std::io::Error::last_os_error().kind() == std::io::ErrorKind::PermissionDenied
        {
            return StorageError::Permission(path.to_path_buf());
        }
    }
    StorageError::Open(path.to_path_buf(), err)
}
