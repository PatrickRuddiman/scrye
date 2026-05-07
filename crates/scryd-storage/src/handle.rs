//! Connection topology: one mutex-guarded write connection plus a small pool
//! of read-only connections, both opened once at startup.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::db::{self, StorageError};

/// Cheaply clonable handle to the daemon's `meta.sqlite`. Internally backed
/// by an `Arc` so cloning shares the same connection set across tasks.
#[derive(Clone)]
pub struct StorageHandle {
    inner: Arc<Inner>,
}

struct Inner {
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    reader_counter: AtomicUsize,
}

impl StorageHandle {
    /// Open `meta.sqlite` under `data_dir` and warm `read_pool_size`
    /// additional read-only connections. Migrations run on the writer
    /// before this returns.
    pub fn open(data_dir: &Path, read_pool_size: usize) -> Result<Self, StorageError> {
        std::fs::create_dir_all(data_dir).map_err(|e| {
            StorageError::Open(data_dir.to_path_buf(), rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?;
        let path = data_dir.join("meta.sqlite");

        let writer = db::open(&path)?;
        let mut readers = Vec::with_capacity(read_pool_size);
        for _ in 0..read_pool_size.max(1) {
            readers.push(Mutex::new(db::open_read_only(&path)?));
        }

        Ok(Self {
            inner: Arc::new(Inner {
                writer: Mutex::new(writer),
                readers,
                reader_counter: AtomicUsize::new(0),
            }),
        })
    }

    /// Run a closure with exclusive access to the writer connection.
    pub async fn with_writer<F, R>(&self, f: F) -> Result<R, StorageError>
    where
        F: FnOnce(&mut Connection) -> Result<R, StorageError>,
    {
        let mut guard = self.inner.writer.lock().await;
        f(&mut guard)
    }

    /// Run a closure against one of the read pool connections, picked
    /// round-robin.
    pub async fn with_reader<F, R>(&self, f: F) -> Result<R, StorageError>
    where
        F: FnOnce(&Connection) -> Result<R, StorageError>,
    {
        let n = self.inner.readers.len();
        let i = self.inner.reader_counter.fetch_add(1, Ordering::Relaxed) % n;
        let guard = self.inner.readers[i].lock().await;
        f(&guard)
    }
}
