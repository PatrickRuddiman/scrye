//! Atomic upsert of an `[[accounts]]` block in `config.toml`. Uses
//! `toml_edit` so existing comments and key ordering survive when an
//! account's password is rotated in place.

use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use toml_edit::{value, Array, ArrayOfTables, DocumentMut, Item, Table};

use crate::exit::ExitCode;

#[derive(Debug, Clone)]
pub struct AccountEntry {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub folders: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("read existing config {0}: {1}")]
    Read(std::path::PathBuf, #[source] std::io::Error),
    #[error("parse existing config {0}: {1}")]
    Parse(std::path::PathBuf, #[source] toml_edit::TomlError),
    #[error("write config tmp {0}: {1}")]
    Write(std::path::PathBuf, #[source] std::io::Error),
    #[error("rename tmp into final config: {0}")]
    Rename(#[source] std::io::Error),
    #[error("create config directory: {0}")]
    Mkdir(#[source] std::io::Error),
    #[error("no account with id `{account_id}` in config")]
    AccountNotFound { account_id: String },
}

impl WriteError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::AccountNotFound { .. } => ExitCode::BadInput,
            _ => ExitCode::ConfigError,
        }
    }
}

/// Atomically upsert `entry` into `config_path`. The function preserves
/// existing comments and ordering when the entry already exists.
pub fn upsert_account(config_path: &Path, entry: &AccountEntry) -> Result<(), WriteError> {
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(WriteError::Mkdir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    parent,
                    std::fs::Permissions::from_mode(0o700),
                );
            }
        }
    }

    let existing = match std::fs::read_to_string(config_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(WriteError::Read(config_path.to_path_buf(), e)),
    };
    let mut doc: DocumentMut = existing
        .parse()
        .map_err(|e| WriteError::Parse(config_path.to_path_buf(), e))?;

    upsert_into_doc(&mut doc, entry);

    let serialized = doc.to_string();
    let tmp_path = with_tmp_suffix(config_path);
    write_atomic(&tmp_path, config_path, serialized.as_bytes())?;
    Ok(())
}

/// Atomically replace the `password` of the existing `[[accounts]]`
/// entry whose id is `account_id`. Returns `AccountNotFound` if no
/// such entry exists. Other fields and operator comments are
/// preserved byte-for-byte.
pub fn rotate_password(
    config_path: &Path,
    account_id: &str,
    new_password: &str,
) -> Result<(), WriteError> {
    let existing = std::fs::read_to_string(config_path)
        .map_err(|e| WriteError::Read(config_path.to_path_buf(), e))?;
    let mut doc: DocumentMut = existing
        .parse()
        .map_err(|e| WriteError::Parse(config_path.to_path_buf(), e))?;

    let arr = doc
        .get_mut("accounts")
        .and_then(|i| i.as_array_of_tables_mut())
        .ok_or_else(|| WriteError::AccountNotFound {
            account_id: account_id.to_string(),
        })?;
    let mut found = false;
    for table in arr.iter_mut() {
        let id = table.get("id").and_then(|i| i.as_str()).unwrap_or("");
        if id == account_id {
            table["password"] = value(new_password);
            found = true;
            break;
        }
    }
    if !found {
        return Err(WriteError::AccountNotFound {
            account_id: account_id.to_string(),
        });
    }

    let serialized = doc.to_string();
    let tmp_path = with_tmp_suffix(config_path);
    write_atomic(&tmp_path, config_path, serialized.as_bytes())?;
    Ok(())
}

fn upsert_into_doc(doc: &mut DocumentMut, entry: &AccountEntry) {
    // Find an existing [[accounts]] entry with the same id.
    let need_new_array = !doc.contains_key("accounts");
    if need_new_array {
        doc.insert("accounts", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let arr = doc
        .get_mut("accounts")
        .and_then(|i| i.as_array_of_tables_mut())
        .expect("accounts is an array-of-tables");

    let mut found = false;
    for table in arr.iter_mut() {
        let existing_id = table.get("id").and_then(|i| i.as_str()).unwrap_or("");
        if existing_id == entry.id {
            apply_entry_to_table(table, entry);
            found = true;
            break;
        }
    }

    if !found {
        let mut t = Table::new();
        // Insert in canonical order so newly-added accounts read consistently.
        t.insert("id", value(entry.id.as_str()));
        t.insert("host", value(entry.host.as_str()));
        t.insert("port", value(entry.port as i64));
        t.insert("user", value(entry.user.as_str()));
        t.insert("password", value(entry.password.as_str()));
        t.insert("folders", folders_array(entry));
        arr.push(t);
    }
}

fn apply_entry_to_table(table: &mut Table, entry: &AccountEntry) {
    table["host"] = value(entry.host.as_str());
    table["port"] = value(entry.port as i64);
    table["user"] = value(entry.user.as_str());
    table["password"] = value(entry.password.as_str());
    table["folders"] = folders_array(entry);
}

fn folders_array(entry: &AccountEntry) -> Item {
    let mut arr = Array::new();
    for f in &entry.folders {
        arr.push(f.as_str());
    }
    value(arr)
}

fn with_tmp_suffix(p: &Path) -> std::path::PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(".tmp");
    std::path::PathBuf::from(s)
}

fn write_atomic(tmp: &Path, final_path: &Path, bytes: &[u8]) -> Result<(), WriteError> {
    use std::io::Write as _;
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true).truncate(true).mode(0o600);
    let mut f = opts
        .open(tmp)
        .map_err(|e| WriteError::Write(tmp.to_path_buf(), e))?;
    f.write_all(bytes)
        .map_err(|e| WriteError::Write(tmp.to_path_buf(), e))?;
    f.sync_all()
        .map_err(|e| WriteError::Write(tmp.to_path_buf(), e))?;
    drop(f);
    std::fs::rename(tmp, final_path).map_err(WriteError::Rename)?;
    Ok(())
}
