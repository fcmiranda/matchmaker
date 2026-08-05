use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use redb::{Database, ReadableTable, TableDefinition};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

pub const FRECENCY_TABLE: TableDefinition<&str, &str> = TableDefinition::new("frecency_v1");

/// Access record for a given path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrecencyRecord {
    pub path: String,
    pub count: u64,
    pub last_accessed: u64, // Unix timestamp in seconds
    pub timestamps: Vec<u64>, // Recent access timestamps
}

impl FrecencyRecord {
    pub fn new(path: String) -> Self {
        Self {
            path,
            count: 0,
            last_accessed: 0,
            timestamps: Vec::new(),
        }
    }

    pub fn record_access(&mut self, now: u64) {
        self.count += 1;
        self.last_accessed = now;
        self.timestamps.push(now);

        // Keep at most 50 recent timestamps
        if self.timestamps.len() > 50 {
            let cutoff = now.saturating_sub(7_776_000); // 90 days
            self.timestamps.retain(|&ts| ts >= cutoff);
            if self.timestamps.len() > 50 {
                let start_idx = self.timestamps.len() - 50;
                self.timestamps.drain(0..start_idx);
            }
        }
    }

    /// Calculate Frecency Score based on exponential decay weights:
    /// - Age < 1 hour (3600s): Weight 100
    /// - Age < 1 day (86400s): Weight 80
    /// - Age < 1 week (604800s): Weight 40
    /// - Age < 1 month (2592000s): Weight 20
    /// - Age >= 1 month: Weight 10
    pub fn calculate_score(&self, now: u64) -> u32 {
        let mut score: u32 = 0;
        for &ts in &self.timestamps {
            let age = now.saturating_sub(ts);
            let weight = if age < 3600 {
                100
            } else if age < 86400 {
                80
            } else if age < 604800 {
                40
            } else if age < 2592000 {
                20
            } else {
                10
            };
            score = score.saturating_add(weight);
        }
        score
    }
}

/// In-memory snapshot for zero-allocation fast lookup during active fuzzy search.
#[derive(Debug, Clone, Default)]
pub struct FrecencySnapshot {
    pub scores: FxHashMap<String, u32>,
}

impl FrecencySnapshot {
    #[inline]
    pub fn get_bonus(&self, path: &str) -> u32 {
        let clean = clean_path(path);
        self.scores.get(clean).copied().unwrap_or(0)
    }
}

/// Helper function to normalize path strings (trim whitespace, remove trailing slashes).
pub fn clean_path(path: &str) -> &str {
    let p = path.trim();
    if p.len() > 1 {
        p.strip_suffix('/').or_else(|| p.strip_suffix('\\')).unwrap_or(p)
    } else {
        p
    }
}

/// Main Frecency Store wrapping `redb::Database` with thread safety and resilient fallback.
#[derive(Clone)]
pub struct FrecencyStore {
    db: Arc<Option<Database>>,
    pub db_path: Option<PathBuf>,
}

impl std::fmt::Debug for FrecencyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrecencyStore")
            .field("db_path", &self.db_path)
            .field("active", &self.db.is_some())
            .finish()
    }
}

impl FrecencyStore {
    /// Default state directory for matchmaker frecency DB:
    /// `~/.local/state/matchmaker/frecency.redb`
    pub fn default_db_path() -> Option<PathBuf> {
        let dir = dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .map(|d| d.join("matchmaker"))?;
        Some(dir.join("frecency.redb"))
    }

    /// Opens or creates the frecency database at default location with resilient error handling.
    pub fn open() -> Self {
        if let Some(path) = Self::default_db_path() {
            Self::open_at(&path).unwrap_or_else(|err| {
                log::warn!("Failed to open frecency store at {path:?}: {err}. Falling back to in-memory mode.");
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

    /// Opens or creates the frecency database at a specific path.
    pub fn open_at(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let db_res = Database::create(path);
        let db = match db_res {
            Ok(database) => Some(database),
            Err(err) => {
                log::error!("redb error opening {path:?}: {err}. Attempting recovery...");
                // If database corrupt, attempt backup and recreate clean
                let backup_path = path.with_extension(format!("corrupt.{}.bak", current_unix_secs()));
                let _ = fs::rename(path, &backup_path);
                Database::create(path).ok()
            }
        };

        Ok(Self {
            db: Arc::new(db),
            db_path: Some(path.to_path_buf()),
        })
    }

    /// Record access event for a file or directory path. Returns updated score.
    pub fn add(&self, raw_path: &str) -> anyhow::Result<u32> {
        let Some(db) = self.db.as_ref() else {
            return Ok(0);
        };

        let key = clean_path(raw_path);
        let now = current_unix_secs();

        let write_txn = db.begin_write()?;
        let score = {
            let mut table = write_txn.open_table(FRECENCY_TABLE)?;
            let mut record = if let Some(guard) = table.get(key)? {
                let json_str = guard.value();
                serde_json::from_str::<FrecencyRecord>(json_str)
                    .unwrap_or_else(|_| FrecencyRecord::new(key.to_string()))
            } else {
                FrecencyRecord::new(key.to_string())
            };

            record.record_access(now);
            let updated_score = record.calculate_score(now);
            let json_str = serde_json::to_string(&record)?;
            table.insert(key, json_str.as_str())?;
            updated_score
        };

        write_txn.commit()?;
        Ok(score)
    }

    /// Query current calculated frecency score for a path.
    pub fn get_bonus(&self, raw_path: &str) -> u32 {
        let Some(db) = self.db.as_ref() else {
            return 0;
        };

        let key = clean_path(raw_path);
        let now = current_unix_secs();

        let read_txn = match db.begin_read() {
            Ok(t) => t,
            Err(_) => return 0,
        };

        let table = match read_txn.open_table(FRECENCY_TABLE) {
            Ok(t) => t,
            Err(_) => return 0,
        };

        match table.get(key) {
            Ok(Some(guard)) => {
                if let Ok(record) = serde_json::from_str::<FrecencyRecord>(guard.value()) {
                    record.calculate_score(now)
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    /// Retrieve full FrecencyRecord details for a path.
    pub fn rank(&self, raw_path: &str) -> Option<FrecencyRecord> {
        let Some(db) = self.db.as_ref() else {
            return None;
        };

        let key = clean_path(raw_path);
        let read_txn = db.begin_read().ok()?;
        let table = read_txn.open_table(FRECENCY_TABLE).ok()?;
        let guard = table.get(key).ok()??;
        serde_json::from_str::<FrecencyRecord>(guard.value()).ok()
    }

    /// Load all tracked entries into an in-memory snapshot for sub-millisecond lookup.
    pub fn get_snapshot(&self) -> FrecencySnapshot {
        let mut snapshot = FrecencySnapshot::default();
        let Some(db) = self.db.as_ref() else {
            return snapshot;
        };

        let now = current_unix_secs();
        if let Ok(read_txn) = db.begin_read() {
            if let Ok(table) = read_txn.open_table(FRECENCY_TABLE) {
                if let Ok(iter) = table.iter() {
                    for entry in iter.flatten() {
                        let key = entry.0.value();
                        let json_val = entry.1.value();
                        if let Ok(record) = serde_json::from_str::<FrecencyRecord>(json_val) {
                            let score = record.calculate_score(now);
                            snapshot.scores.insert(key.to_string(), score);
                        }
                    }
                }
            }
        }

        snapshot
    }

    /// Retrieve all records stored in database.
    pub fn all_records(&self) -> Vec<FrecencyRecord> {
        let Some(db) = self.db.as_ref() else {
            return Vec::new();
        };

        let mut records = Vec::new();
        if let Ok(read_txn) = db.begin_read() {
            if let Ok(table) = read_txn.open_table(FRECENCY_TABLE) {
                if let Ok(iter) = table.iter() {
                    for entry in iter.flatten() {
                        if let Ok(record) = serde_json::from_str::<FrecencyRecord>(entry.1.value()) {
                            records.push(record);
                        }
                    }
                }
            }
        }
        records
    }

    /// Import an entry with a specified access count/weight into the database.
    pub fn import_entry(&self, raw_path: &str, count: u64) -> anyhow::Result<()> {
        let Some(db) = self.db.as_ref() else {
            return Ok(());
        };

        let key = clean_path(raw_path);
        let now = current_unix_secs();

        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(FRECENCY_TABLE)?;
            let mut record = if let Some(guard) = table.get(key)? {
                let json_str = guard.value();
                serde_json::from_str::<FrecencyRecord>(json_str)
                    .unwrap_or_else(|_| FrecencyRecord::new(key.to_string()))
            } else {
                FrecencyRecord::new(key.to_string())
            };

            let iterations = count.clamp(1, 50);
            for _ in 0..iterations {
                record.record_access(now);
            }

            let json_str = serde_json::to_string(&record)?;
            table.insert(key, json_str.as_str())?;
        }

        write_txn.commit()?;
        Ok(())
    }

    /// Purges all entries whose file/directory path no longer exists on disk.
    pub fn clean_stale(&self) -> anyhow::Result<usize> {
        let Some(db) = self.db.as_ref() else {
            return Ok(0);
        };

        let mut keys_to_remove = Vec::new();
        if let Ok(read_txn) = db.begin_read() {
            if let Ok(table) = read_txn.open_table(FRECENCY_TABLE) {
                if let Ok(iter) = table.iter() {
                    for entry in iter.flatten() {
                        let path_str = entry.0.value();
                        if !Path::new(path_str).exists() {
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
            let mut table = write_txn.open_table(FRECENCY_TABLE)?;
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

    /// Removes a specific path entry from the frecency database. Returns true if key was present.
    pub fn remove(&self, raw_path: &str) -> anyhow::Result<bool> {
        let Some(db) = self.db.as_ref() else {
            return Ok(false);
        };

        let key = clean_path(raw_path);
        let write_txn = db.begin_write()?;
        let removed = {
            let mut table = write_txn.open_table(FRECENCY_TABLE)?;
            table.remove(key)?.is_some()
        };

        write_txn.commit()?;
        Ok(removed)
    }
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_path() {
        assert_eq!(clean_path("/home/user/project/"), "/home/user/project");
        assert_eq!(clean_path("src/lib.rs"), "src/lib.rs");
        assert_eq!(clean_path("/"), "/");
    }

    #[test]
    fn test_frecency_score_decay() {
        let now = 1_000_000;
        let mut rec = FrecencyRecord::new("foo.txt".into());
        rec.record_access(now);
        assert_eq!(rec.calculate_score(now), 100);

        // 2 hours later (7200s) -> weight 80
        rec.timestamps.push(now - 7200);
        assert_eq!(rec.calculate_score(now), 180);

        // 3 days ago -> weight 40
        rec.timestamps.push(now - 3 * 86400);
        assert_eq!(rec.calculate_score(now), 220);
    }

    #[test]
    fn test_store_open_add_rank() -> anyhow::Result<()> {
        let temp_dir = std::env::temp_dir().join("mm_test_frecency");
        let _ = fs::remove_dir_all(&temp_dir);
        let db_path = temp_dir.join("test.redb");

        let store = FrecencyStore::open_at(&db_path)?;
        let path = "/test/path/file.rs";

        let score1 = store.add(path)?;
        assert!(score1 >= 100);

        let rank_res = store.rank(path);
        assert!(rank_res.is_some());
        let record = rank_res.unwrap();
        assert_eq!(record.count, 1);
        assert_eq!(record.path, "/test/path/file.rs");

        let score2 = store.add(path)?;
        assert!(score2 > score1);

        let snapshot = store.get_snapshot();
        assert_eq!(snapshot.get_bonus(path), score2);

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    #[test]
    fn test_store_import_and_clean_stale() -> anyhow::Result<()> {
        let temp_dir = std::env::temp_dir().join("mm_test_frecency_clean");
        let _ = fs::remove_dir_all(&temp_dir);
        let db_path = temp_dir.join("test.redb");

        let store = FrecencyStore::open_at(&db_path)?;
        let existing_path = temp_dir.to_str().unwrap();
        let non_existing_path = "/non/existent/path/for/mm/test.rs";

        store.import_entry(existing_path, 3)?;
        store.import_entry(non_existing_path, 5)?;

        assert!(store.get_bonus(existing_path) > 0);
        assert!(store.get_bonus(non_existing_path) > 0);

        let cleaned = store.clean_stale()?;
        assert_eq!(cleaned, 1);

        assert!(store.get_bonus(existing_path) > 0);
        assert_eq!(store.get_bonus(non_existing_path), 0);

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    #[test]
    fn test_store_remove() -> anyhow::Result<()> {
        let temp_dir = std::env::temp_dir().join("mm_test_frecency_remove");
        let _ = fs::remove_dir_all(&temp_dir);
        let db_path = temp_dir.join("test.redb");

        let store = FrecencyStore::open_at(&db_path)?;
        let path = "/path/to/remove";

        store.add(path)?;
        assert!(store.get_bonus(path) > 0);

        let removed = store.remove(path)?;
        assert!(removed);
        assert_eq!(store.get_bonus(path), 0);

        let removed_again = store.remove(path)?;
        assert!(!removed_again);

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }
}
