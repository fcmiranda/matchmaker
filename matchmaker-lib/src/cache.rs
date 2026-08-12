use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

pub const DIR_CACHE_TABLE: TableDefinition<&str, &str> = TableDefinition::new("dir_cache_v1");

/// Cache record for a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirCacheRecord {
    pub root: String,
    pub timestamp: u64, // Unix timestamp in seconds
    pub items: Vec<String>,
}

impl DirCacheRecord {
    pub fn new(root: String, items: Vec<String>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            root,
            timestamp,
            items,
        }
    }

    pub fn age_secs(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.timestamp)
    }
}

/// Directory Listing Cache Store powered by `redb`.
#[derive(Clone)]
pub struct DirCacheStore {
    db: Arc<Option<Database>>,
    pub db_path: Option<PathBuf>,
}

impl std::fmt::Debug for DirCacheStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirCacheStore")
            .field("db_path", &self.db_path)
            .field("active", &self.db.is_some())
            .finish()
    }
}

impl DirCacheStore {
    /// Default state path: `~/.local/state/matchmaker/dir_cache.redb`
    pub fn default_db_path() -> Option<PathBuf> {
        let dir = dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .map(|d| d.join("matchmaker"))?;
        Some(dir.join("dir_cache.redb"))
    }

    /// Opens or creates the directory cache store at default location.
    pub fn open() -> Self {
        if let Some(path) = Self::default_db_path() {
            Self::open_at(&path).unwrap_or_else(|err| {
                log::warn!("Failed to open dir cache store at {path:?}: {err}. Falling back to in-memory mode.");
                Self {
                    db: Arc::new(None),
                    db_path: Some(path),
                }
            })
        } else {
            Self {
                db: Arc::new(None),
                db_path: None,
            }
        }
    }

    /// Opens or creates the directory cache store at a specific path.
    pub fn open_at(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let db_res = Database::create(path);
        let db = match db_res {
            Ok(database) => Some(database),
            Err(err) => {
                log::error!("redb error opening dir cache at {path:?}: {err}. Recreating...");
                let backup_path = path.with_extension("corrupt.bak");
                let _ = fs::rename(path, &backup_path);
                Database::create(path).ok()
            }
        };

        Ok(Self {
            db: Arc::new(db),
            db_path: Some(path.to_path_buf()),
        })
    }

    /// Retrieves cached directory listing record for a root directory path.
    pub fn get(&self, raw_root: &str) -> Option<DirCacheRecord> {
        let Some(db) = self.db.as_ref() else {
            return None;
        };

        let key_str = crate::frecency::normalize_path(raw_root);
        if key_str.is_empty() {
            return None;
        }

        let read_txn = db.begin_read().ok()?;
        let table = read_txn.open_table(DIR_CACHE_TABLE).ok()?;
        let guard = table.get(key_str.as_str()).ok()??;
        serde_json::from_str::<DirCacheRecord>(guard.value()).ok()
    }

    /// Stores/updates cached directory listing record for a root directory path.
    pub fn put(&self, raw_root: &str, items: Vec<String>) -> anyhow::Result<()> {
        let Some(db) = self.db.as_ref() else {
            return Ok(());
        };

        let key_str = crate::frecency::normalize_path(raw_root);
        if key_str.is_empty() {
            return Ok(());
        }

        let record = DirCacheRecord::new(key_str.clone(), items);
        let json_str = serde_json::to_string(&record)?;

        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(DIR_CACHE_TABLE)?;
            table.insert(key_str.as_str(), json_str.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Removes a root path entry from the directory cache database.
    pub fn remove(&self, raw_root: &str) -> anyhow::Result<bool> {
        let Some(db) = self.db.as_ref() else {
            return Ok(false);
        };

        let key_str = crate::frecency::normalize_path(raw_root);
        if key_str.is_empty() {
            return Ok(false);
        }

        let write_txn = db.begin_write()?;
        let removed = {
            let mut table = write_txn.open_table(DIR_CACHE_TABLE)?;
            table.remove(key_str.as_str())?.is_some()
        };
        write_txn.commit()?;
        Ok(removed)
    }

    /// Purges entries whose root path no longer exists on disk.
    pub fn clean_stale(&self) -> anyhow::Result<usize> {
        let Some(db) = self.db.as_ref() else {
            return Ok(0);
        };

        let mut keys_to_remove = Vec::new();
        if let Ok(read_txn) = db.begin_read() {
            if let Ok(table) = read_txn.open_table(DIR_CACHE_TABLE) {
                if let Ok(iter) = table.iter() {
                    for entry in iter.flatten() {
                        let path_str = entry.0.value();
                        let p = Path::new(path_str);
                        if !p.exists() {
                            keys_to_remove.push(path_str.to_string());
                        }
                    }
                }
            }
        }

        if keys_to_remove.is_empty() {
            return Ok(0);
        }

        let write_txn = db.begin_write()?;
        let removed_count = {
            let mut table = write_txn.open_table(DIR_CACHE_TABLE)?;
            let mut count = 0;
            for key in &keys_to_remove {
                if table.remove(key.as_str())?.is_some() {
                    count += 1;
                }
            }
            count
        };

        write_txn.commit()?;
        Ok(removed_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dir_cache_store_put_get_remove() -> anyhow::Result<()> {
        let temp_dir = std::env::temp_dir().join("mm_test_dir_cache");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir)?;
        let db_path = temp_dir.join("cache.redb");

        let store = DirCacheStore::open_at(&db_path)?;
        let root_str = temp_dir.to_str().unwrap();

        assert!(store.get(root_str).is_none());

        let items = vec!["src/main.rs".to_string(), "README.md".to_string()];
        store.put(root_str, items.clone())?;

        let cached = store.get(root_str);
        assert!(cached.is_some());
        let rec = cached.unwrap();
        assert_eq!(rec.items, items);

        let removed = store.remove(root_str)?;
        assert!(removed);
        assert!(store.get(root_str).is_none());

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }
}
