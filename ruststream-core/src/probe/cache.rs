//! Media metadata cache using redb (embedded KV store)
//!
//! Provides persistent caching of probe results to avoid re-probing same files.
//! Implements LRU eviction when cache exceeds max_size_mb.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::RwLock;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Serialize, Deserialize};
use crate::probe::FullMetadata;
use crate::core::{MediaError, MediaErrorCode, MediaResult};

/// Table definition for redb
const METADATA_TABLE: TableDefinition<&str, &str> = TableDefinition::new("metadata");

/// Maximum cache size in MB
const DEFAULT_MAX_SIZE_MB: u64 = 100;

/// Target size after eviction (evict to 80% of max)
const EVICTION_TARGET_RATIO: f64 = 0.8;

/// LRU index for tracking access order
#[derive(Debug, Clone)]
struct LruIndex {
    order: VecDeque<String>,
    size_bytes: u64,
}

impl LruIndex {
    fn new() -> Self {
        Self {
            order: VecDeque::new(),
            size_bytes: 0,
        }
    }

    /// Record access (move to front)
    fn touch(&mut self, key: &str) {
        // Remove if exists, then push front
        self.order.retain(|k| k != key);
        self.order.push_front(key.to_string());
    }

    /// Add new entry
    fn insert(&mut self, key: &str, size_bytes: u64) {
        self.touch(key);
        self.size_bytes += size_bytes;
    }

    /// Remove entry
    fn remove(&mut self, key: &str) -> Option<u64> {
        let was_present = self.order.iter().any(|k| k == key);
        self.order.retain(|k| k != key);
        if was_present {
            // Approximate: we don't track per-entry size, so return 0
            // Real size is tracked via stats() method
            Some(0)
        } else {
            None
        }
    }

    /// Get keys to evict (oldest entries)
    fn keys_to_evict(&self, count: usize) -> Vec<String> {
        self.order.iter().rev().take(count).cloned().collect()
    }

    /// Clear the index
    fn clear(&mut self) {
        self.order.clear();
        self.size_bytes = 0;
    }

    fn len(&self) -> usize {
        self.order.len()
    }
}

/// Media metadata cache with LRU eviction
pub struct MediaCache {
    db: Arc<RwLock<Option<Database>>>,
    path: PathBuf,
    max_size_mb: u64,
    lru: Arc<RwLock<LruIndex>>,
}

impl MediaCache {
    /// Open cache at default location
    pub fn open_default() -> MediaResult<Self> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("./.cache"))
            .join("ruststream");
        
        std::fs::create_dir_all(&cache_dir).map_err(|e| {
            MediaError::new(MediaErrorCode::CacheOpenFailed, e.to_string())
        })?;
        
        Self::open(&cache_dir.join("metadata.redb"), DEFAULT_MAX_SIZE_MB)
    }
    
    /// Open cache at specified path
    pub fn open(path: &Path, max_size_mb: u64) -> MediaResult<Self> {
        let db = Database::create(path)
            .map_err(|e| MediaError::new(
                MediaErrorCode::CacheOpenFailed,
                format!("Failed to open cache database: {}", e)
            ))?;
        
        // Create table if not exists
        let write_txn = db.begin_write()
            .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
        
        {
            let _ = write_txn.open_table(METADATA_TABLE)
                .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
        }
        
        write_txn.commit()
            .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
        
        Ok(Self {
            db: Arc::new(RwLock::new(Some(db))),
            path: path.to_path_buf(),
            max_size_mb,
            lru: Arc::new(RwLock::new(LruIndex::new())),
        })
    }
    
    /// Create in-memory cache (for testing)
    pub fn in_memory() -> Self {
        Self {
            db: Arc::new(RwLock::new(None)),
            path: PathBuf::from(":memory:"),
            max_size_mb: 10,
            lru: Arc::new(RwLock::new(LruIndex::new())),
        }
    }
    
    /// Enforce size limit by evicting oldest entries
    fn enforce_size_limit(&self) -> MediaResult<()> {
        let db_guard = self.db.read();
        let Some(db) = db_guard.as_ref() else {
            return Ok(());
        };
        
        let max_bytes = self.max_size_mb * 1024 * 1024;
        let stats = self.stats();
        let current_bytes = stats.approx_size_bytes;
        
        if current_bytes <= max_bytes {
            return Ok(());
        }
        
        // Calculate how many entries to evict (target 80% of max)
        let target_bytes = (max_bytes as f64 * EVICTION_TARGET_RATIO) as u64;
        let bytes_to_free = current_bytes - target_bytes;
        
        log::info!(
            "Cache size {} MB exceeds limit {} MB, evicting entries...",
            current_bytes / (1024 * 1024),
            self.max_size_mb
        );
        
        // Get keys to evict (oldest first)
        let lru_guard = self.lru.read();
        let keys_to_evict: Vec<String> = lru_guard.keys_to_evict(100); // Evict up to 100 at a time
        drop(lru_guard);
        
        if keys_to_evict.is_empty() {
            return Ok(());
        }
        
        // Delete entries from database
        let write_txn = db.begin_write()
            .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
        
        let evicted_count = {
            let mut table = write_txn.open_table(METADATA_TABLE)
                .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
            
            let mut count = 0;
            for key in &keys_to_evict {
                if table.remove(key.as_str()).is_ok() {
                    count += 1;
                }
            }
            count
        };
        
        write_txn.commit()
            .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
        
        // Update LRU index
        let mut lru_guard = self.lru.write();
        for key in &keys_to_evict {
            lru_guard.remove(key);
        }
        
        log::info!("Evicted {} cache entries, approx freed {} MB", 
            evicted_count, bytes_to_free / (1024 * 1024));
        
        Ok(())
    }
    
    /// Get metadata from cache
    pub fn get(&self, path: &str) -> MediaResult<Option<FullMetadata>> {
        let db_guard = self.db.read();
        let Some(db) = db_guard.as_ref() else {
            return Ok(None);
        };
        
        let read_txn = db.begin_read()
            .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
        
        let table = read_txn.open_table(METADATA_TABLE)
            .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
        
        match table.get(path) {
            Ok(Some(value)) => {
                let json = value.value();
                let metadata: FullMetadata = serde_json::from_str(json)
                    .map_err(|e| MediaError::new(
                        MediaErrorCode::CacheWriteFailed,
                        format!("Failed to deserialize cached metadata: {}", e)
                    ))?;
                
                // Update LRU index
                self.lru.write().touch(path);
                
                Ok(Some(metadata))
            }
            Ok(None) | Err(_) => Ok(None),
        }
    }
    
    /// Put metadata in cache
    pub fn put(&self, path: &str, metadata: &FullMetadata) -> MediaResult<()> {
        let db_guard = self.db.read();
        let Some(db) = db_guard.as_ref() else {
            return Ok(()); // In-memory cache, skip
        };
        
        let json = serde_json::to_string(metadata)
            .map_err(|e| MediaError::new(
                MediaErrorCode::CacheWriteFailed,
                format!("Failed to serialize metadata: {}", e)
            ))?;
        
        let entry_size = json.len() as u64;
        
        let write_txn = db.begin_write()
            .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
        
        {
            let mut table = write_txn.open_table(METADATA_TABLE)
                .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
            
            table.insert(path, json.as_str())
                .map_err(|e| MediaError::new(
                    MediaErrorCode::CacheWriteFailed,
                    format!("Failed to insert into cache: {}", e)
                ))?;
        }
        
        write_txn.commit()
            .map_err(|e| MediaError::new(MediaErrorCode::CacheWriteFailed, e.to_string()))?;
        
        // Update LRU index and enforce size limit
        self.lru.write().insert(path, entry_size);
        self.enforce_size_limit()?;
        
        Ok(())
    }
    
    /// Get or probe metadata
    pub fn get_or_probe(&self, path: &str) -> MediaResult<FullMetadata> {
        // Try cache first
        if let Some(metadata) = self.get(path)? {
            log::debug!("Cache hit for {}", path);
            return Ok(metadata);
        }
        
        // Probe and cache
        log::debug!("Cache miss for {}, probing...", path);
        let metadata = crate::probe::probe_full(path)?;
        self.put(path, &metadata)?;
        
        Ok(metadata)
    }
    
    /// Clear cache
    pub fn clear(&self) -> MediaResult<()> {
        let db_guard = self.db.read();
        let Some(db) = db_guard.as_ref() else {
            return Ok(());
        };
        
        let write_txn = db.begin_write()
            .map_err(|e| MediaError::new(MediaErrorCode::CacheClearFailed, e.to_string()))?;
        
        {
            let mut table = write_txn.open_table(METADATA_TABLE)
                .map_err(|e| MediaError::new(MediaErrorCode::CacheClearFailed, e.to_string()))?;
            
            // Delete all entries
            let len = table.len()
                .map_err(|e| MediaError::new(MediaErrorCode::CacheClearFailed, e.to_string()))?;
            
            for i in 0..len {
                if let Ok(Some(entry)) = table.get_range(i..i+1)
                    .and_then(|mut r| r.next())
                {
                    let key = entry.0.value().to_string();
                    let _ = table.remove(&key);
                }
            }
        }
        
        write_txn.commit()
            .map_err(|e| MediaError::new(MediaErrorCode::CacheClearFailed, e.to_string()))?;
        
        // Clear LRU index
        self.lru.write().clear();
        
        Ok(())
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let db_guard = self.db.read();
        let Some(db) = db_guard.as_ref() else {
            return CacheStats::default();
        };
        
        let read_txn = match db.begin_read() {
            Ok(txn) => txn,
            Err(_) => return CacheStats::default(),
        };
        
        let table = match read_txn.open_table(METADATA_TABLE) {
            Ok(t) => t,
            Err(_) => return CacheStats::default(),
        };
        
        let entry_count = table.len().unwrap_or(0);
        
        // Calculate approximate size by iterating all entries
        let mut approx_size_bytes: u64 = 0;
        if let Ok(range) = table.range::<&str>(..) {
            for entry in range.flatten() {
                let value = entry.1.value();
                approx_size_bytes += value.len() as u64;
            }
        }
        
        CacheStats {
            entry_count,
            max_size_mb: self.max_size_mb,
            path: self.path.clone(),
            approx_size_bytes,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheStats {
    pub entry_count: usize,
    pub max_size_mb: u64,
    pub path: PathBuf,
    /// Approximate total size of cached data in bytes
    #[serde(default)]
    pub approx_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_in_memory() {
        let cache = MediaCache::in_memory();
        assert_eq!(cache.stats().entry_count, 0);
    }
    
    #[test]
    fn test_cache_get_miss() {
        let cache = MediaCache::in_memory();
        let result = cache.get("/nonexistent").unwrap();
        assert!(result.is_none());
    }
}
