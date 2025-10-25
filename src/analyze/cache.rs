use crate::analyze::{ProjectType, SymbolInfo};
use crate::error::{QuickctxError, Result};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Cache entry for symbol extraction results
#[derive(Debug, Serialize, Deserialize)]
struct SymbolCacheEntry {
    file_path: PathBuf,
    mtime_secs: u64,
    mtime_nanos: u32,
    file_size: u64,
    symbols: Vec<SymbolInfo>,
    project_type: ProjectType,
}

/// Cache entry for external type definitions
#[derive(Debug, Serialize, Deserialize)]
struct ExternalTypeCacheEntry {
    file_path: PathBuf,
    mtime_secs: u64,
    mtime_nanos: u32,
    file_size: u64,
    symbols: Vec<SymbolInfo>,
}

/// Cache manager for symbol extraction and external types
pub struct SymbolCache {
    cache_root: PathBuf,
    symbols_dir: PathBuf,
    external_dir: PathBuf,
}

impl SymbolCache {
    /// Create a new cache instance with the given cache directory
    pub fn new(cache_dir: Option<PathBuf>) -> Result<Self> {
        let cache_root = if let Some(dir) = cache_dir {
            dir
        } else {
            // Use XDG-compliant cache directory
            Self::default_cache_dir()?
        };

        let symbols_dir = cache_root.join("symbols");
        let external_dir = cache_root.join("external");

        // Create cache directories if they don't exist
        fs::create_dir_all(&symbols_dir).map_err(QuickctxError::Io)?;
        fs::create_dir_all(&external_dir).map_err(QuickctxError::Io)?;

        tracing::debug!("Cache initialized at: {}", cache_root.display());

        Ok(Self {
            cache_root,
            symbols_dir,
            external_dir,
        })
    }

    /// Get the default cache directory (~/.cache/quickctx/analyze)
    fn default_cache_dir() -> Result<PathBuf> {
        let cache_base = if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(xdg_cache)
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".cache")
        } else {
            return Err(QuickctxError::Config(
                "Could not determine cache directory: HOME not set".to_string(),
            ));
        };

        Ok(cache_base.join("quickctx").join("analyze"))
    }

    /// Generate cache key from file path
    fn cache_key(file_path: &Path) -> String {
        let mut hasher = DefaultHasher::new();
        file_path.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Get cached symbols for a file
    pub fn get_symbols(
        &self,
        file_path: &Path,
        project_type: ProjectType,
    ) -> Result<Option<Vec<SymbolInfo>>> {
        let key = Self::cache_key(file_path);
        let cache_file = self.symbols_dir.join(&key).join("cache.json");

        if !cache_file.exists() {
            tracing::debug!("Cache miss for {}: no cache file", file_path.display());
            return Ok(None);
        }

        // Read cache entry
        let cache_json = fs::read_to_string(&cache_file).map_err(|e| {
            tracing::warn!("Failed to read cache file: {}", e);
            QuickctxError::Io(e)
        })?;

        let entry: SymbolCacheEntry = serde_json::from_str(&cache_json).map_err(|e| {
            tracing::warn!("Failed to parse cache file: {}", e);
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid cache format: {}", e),
            ))
        })?;

        // Validate cache entry
        if !self.is_valid_symbol_cache(&entry, file_path, project_type)? {
            tracing::debug!("Cache miss for {}: validation failed", file_path.display());
            return Ok(None);
        }

        tracing::info!("Cache hit for {}", file_path.display());
        Ok(Some(entry.symbols))
    }

    /// Save symbols to cache
    pub fn save_symbols(
        &self,
        file_path: &Path,
        symbols: Vec<SymbolInfo>,
        project_type: ProjectType,
    ) -> Result<()> {
        let metadata = fs::metadata(file_path).map_err(QuickctxError::Io)?;
        let mtime = metadata
            .modified()
            .map_err(QuickctxError::Io)?
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| {
                QuickctxError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid mtime: {}", e),
                ))
            })?;

        let entry = SymbolCacheEntry {
            file_path: file_path.to_path_buf(),
            mtime_secs: mtime.as_secs(),
            mtime_nanos: mtime.subsec_nanos(),
            file_size: metadata.len(),
            symbols,
            project_type,
        };

        let key = Self::cache_key(file_path);
        let cache_dir = self.symbols_dir.join(&key);
        fs::create_dir_all(&cache_dir).map_err(QuickctxError::Io)?;

        let cache_file = cache_dir.join("cache.json");
        let cache_json = serde_json::to_string_pretty(&entry).map_err(|e| {
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize cache: {}", e),
            ))
        })?;

        fs::write(&cache_file, cache_json).map_err(QuickctxError::Io)?;
        tracing::debug!("Cached symbols for {}", file_path.display());

        Ok(())
    }

    /// Get cached external type definitions
    pub fn get_external(&self, file_path: &Path) -> Result<Option<Vec<SymbolInfo>>> {
        let key = Self::cache_key(file_path);
        let cache_file = self.external_dir.join(&key).join("cache.json");

        if !cache_file.exists() {
            tracing::debug!(
                "External cache miss for {}: no cache file",
                file_path.display()
            );
            return Ok(None);
        }

        // Read cache entry
        let cache_json = fs::read_to_string(&cache_file).map_err(|e| {
            tracing::warn!("Failed to read external cache file: {}", e);
            QuickctxError::Io(e)
        })?;

        let entry: ExternalTypeCacheEntry = serde_json::from_str(&cache_json).map_err(|e| {
            tracing::warn!("Failed to parse external cache file: {}", e);
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid cache format: {}", e),
            ))
        })?;

        // Validate cache entry
        if !self.is_valid_external_cache(&entry, file_path)? {
            tracing::debug!(
                "External cache miss for {}: validation failed",
                file_path.display()
            );
            return Ok(None);
        }

        tracing::info!("External cache hit for {}", file_path.display());
        Ok(Some(entry.symbols))
    }

    /// Save external type definitions to cache
    pub fn save_external(&self, file_path: &Path, symbols: Vec<SymbolInfo>) -> Result<()> {
        let metadata = fs::metadata(file_path).map_err(QuickctxError::Io)?;
        let mtime = metadata
            .modified()
            .map_err(QuickctxError::Io)?
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| {
                QuickctxError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid mtime: {}", e),
                ))
            })?;

        let entry = ExternalTypeCacheEntry {
            file_path: file_path.to_path_buf(),
            mtime_secs: mtime.as_secs(),
            mtime_nanos: mtime.subsec_nanos(),
            file_size: metadata.len(),
            symbols,
        };

        let key = Self::cache_key(file_path);
        let cache_dir = self.external_dir.join(&key);
        fs::create_dir_all(&cache_dir).map_err(QuickctxError::Io)?;

        let cache_file = cache_dir.join("cache.json");
        let cache_json = serde_json::to_string_pretty(&entry).map_err(|e| {
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize external cache: {}", e),
            ))
        })?;

        fs::write(&cache_file, cache_json).map_err(QuickctxError::Io)?;
        tracing::debug!("Cached external types for {}", file_path.display());

        Ok(())
    }

    /// Clear all cached data
    pub fn clear(&self) -> Result<()> {
        if self.cache_root.exists() {
            fs::remove_dir_all(&self.cache_root).map_err(QuickctxError::Io)?;
            tracing::info!("Cache cleared: {}", self.cache_root.display());

            // Recreate the directories
            fs::create_dir_all(&self.symbols_dir).map_err(QuickctxError::Io)?;
            fs::create_dir_all(&self.external_dir).map_err(QuickctxError::Io)?;
        }
        Ok(())
    }

    /// Validate symbol cache entry against current file state
    fn is_valid_symbol_cache(
        &self,
        entry: &SymbolCacheEntry,
        file_path: &Path,
        project_type: ProjectType,
    ) -> Result<bool> {
        // Check project type matches
        if entry.project_type != project_type {
            tracing::debug!(
                "Cache invalid: project type mismatch (cached: {:?}, current: {:?})",
                entry.project_type,
                project_type
            );
            return Ok(false);
        }

        // Check file exists and get current metadata
        let metadata = match fs::metadata(file_path) {
            Ok(m) => m,
            Err(_) => {
                tracing::debug!("Cache invalid: file no longer exists");
                return Ok(false);
            }
        };

        // Check file size
        if metadata.len() != entry.file_size {
            tracing::debug!(
                "Cache invalid: file size changed (cached: {}, current: {})",
                entry.file_size,
                metadata.len()
            );
            return Ok(false);
        }

        // Check modification time
        let current_mtime = metadata
            .modified()
            .map_err(QuickctxError::Io)?
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| {
                QuickctxError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid mtime: {}", e),
                ))
            })?;

        if current_mtime.as_secs() != entry.mtime_secs
            || current_mtime.subsec_nanos() != entry.mtime_nanos
        {
            tracing::debug!("Cache invalid: modification time changed");
            return Ok(false);
        }

        Ok(true)
    }

    /// Validate external cache entry against current file state
    fn is_valid_external_cache(
        &self,
        entry: &ExternalTypeCacheEntry,
        file_path: &Path,
    ) -> Result<bool> {
        // Check file exists and get current metadata
        let metadata = match fs::metadata(file_path) {
            Ok(m) => m,
            Err(_) => {
                tracing::debug!("External cache invalid: file no longer exists");
                return Ok(false);
            }
        };

        // Check file size
        if metadata.len() != entry.file_size {
            tracing::debug!(
                "External cache invalid: file size changed (cached: {}, current: {})",
                entry.file_size,
                metadata.len()
            );
            return Ok(false);
        }

        // Check modification time
        let current_mtime = metadata
            .modified()
            .map_err(QuickctxError::Io)?
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| {
                QuickctxError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid mtime: {}", e),
                ))
            })?;

        if current_mtime.as_secs() != entry.mtime_secs
            || current_mtime.subsec_nanos() != entry.mtime_nanos
        {
            tracing::debug!("External cache invalid: modification time changed");
            return Ok(false);
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_cache_key_generation() {
        let path1 = Path::new("/foo/bar.rs");
        let path2 = Path::new("/foo/bar.rs");
        let path3 = Path::new("/foo/baz.rs");

        assert_eq!(SymbolCache::cache_key(path1), SymbolCache::cache_key(path2));
        assert_ne!(SymbolCache::cache_key(path1), SymbolCache::cache_key(path3));
    }

    #[test]
    fn test_cache_roundtrip() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache = SymbolCache::new(Some(temp_dir.path().to_path_buf()))?;

        // Create a test file
        let test_file = temp_dir.path().join("test.rs");
        let mut file = fs::File::create(&test_file).unwrap();
        writeln!(file, "fn main() {{}}").unwrap();

        // Create some test symbols
        let symbols = vec![];

        // Save to cache
        cache.save_symbols(&test_file, symbols.clone(), ProjectType::Rust)?;

        // Retrieve from cache
        let cached = cache.get_symbols(&test_file, ProjectType::Rust)?;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), symbols.len());

        Ok(())
    }

    #[test]
    fn test_cache_invalidation_on_modification() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache = SymbolCache::new(Some(temp_dir.path().to_path_buf()))?;

        // Create a test file
        let test_file = temp_dir.path().join("test.rs");
        let mut file = fs::File::create(&test_file).unwrap();
        writeln!(file, "fn main() {{}}").unwrap();

        // Save to cache
        let symbols = vec![];
        cache.save_symbols(&test_file, symbols.clone(), ProjectType::Rust)?;

        // Verify cache hit
        assert!(cache.get_symbols(&test_file, ProjectType::Rust)?.is_some());

        // Wait a moment and modify the file
        std::thread::sleep(std::time::Duration::from_millis(10));
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&test_file)
            .unwrap();
        writeln!(file, "// modified").unwrap();

        // Cache should be invalid now
        assert!(cache.get_symbols(&test_file, ProjectType::Rust)?.is_none());

        Ok(())
    }

    #[test]
    fn test_external_cache_roundtrip() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache = SymbolCache::new(Some(temp_dir.path().to_path_buf()))?;

        // Create a test file
        let test_file = temp_dir.path().join("external.rs");
        let mut file = fs::File::create(&test_file).unwrap();
        writeln!(file, "struct Foo {{}}").unwrap();

        // Create some test symbols
        let symbols = vec![];

        // Save to cache
        cache.save_external(&test_file, symbols.clone())?;

        // Retrieve from cache
        let cached = cache.get_external(&test_file)?;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), symbols.len());

        Ok(())
    }

    #[test]
    fn test_cache_invalidation_on_size_change() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache = SymbolCache::new(Some(temp_dir.path().to_path_buf()))?;

        // Create a test file
        let test_file = temp_dir.path().join("test.rs");
        fs::write(&test_file, "fn main() {}").unwrap();

        // Save to cache
        let symbols = vec![];
        cache.save_symbols(&test_file, symbols.clone(), ProjectType::Rust)?;

        // Verify cache hit
        assert!(cache.get_symbols(&test_file, ProjectType::Rust)?.is_some());

        // Modify file size without preserving mtime
        fs::write(&test_file, "fn main() {}\n// more content").unwrap();

        // Cache should be invalid due to size change
        assert!(cache.get_symbols(&test_file, ProjectType::Rust)?.is_none());

        Ok(())
    }

    #[test]
    fn test_cache_project_type_mismatch() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache = SymbolCache::new(Some(temp_dir.path().to_path_buf()))?;

        // Create a test file
        let test_file = temp_dir.path().join("test.rs");
        fs::write(&test_file, "fn main() {}").unwrap();

        // Save as Rust
        let symbols = vec![];
        cache.save_symbols(&test_file, symbols.clone(), ProjectType::Rust)?;

        // Try to retrieve as Python - should be cache miss
        assert!(
            cache
                .get_symbols(&test_file, ProjectType::Python)?
                .is_none()
        );

        // But Rust should still work
        assert!(cache.get_symbols(&test_file, ProjectType::Rust)?.is_some());

        Ok(())
    }

    #[test]
    fn test_cache_clear() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache = SymbolCache::new(Some(temp_dir.path().to_path_buf()))?;

        // Create test files and cache them
        let test_file1 = temp_dir.path().join("test1.rs");
        let test_file2 = temp_dir.path().join("test2.rs");
        fs::write(&test_file1, "fn main() {}").unwrap();
        fs::write(&test_file2, "fn foo() {}").unwrap();

        let symbols = vec![];
        cache.save_symbols(&test_file1, symbols.clone(), ProjectType::Rust)?;
        cache.save_external(&test_file2, symbols.clone())?;

        // Verify both are cached
        assert!(cache.get_symbols(&test_file1, ProjectType::Rust)?.is_some());
        assert!(cache.get_external(&test_file2)?.is_some());

        // Clear cache
        cache.clear()?;

        // Both should be cache misses now
        assert!(cache.get_symbols(&test_file1, ProjectType::Rust)?.is_none());
        assert!(cache.get_external(&test_file2)?.is_none());

        Ok(())
    }

    #[test]
    fn test_cache_miss_nonexistent_file() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache = SymbolCache::new(Some(temp_dir.path().to_path_buf()))?;

        let nonexistent = temp_dir.path().join("does_not_exist.rs");

        // Should return None without error
        assert!(
            cache
                .get_symbols(&nonexistent, ProjectType::Rust)?
                .is_none()
        );
        assert!(cache.get_external(&nonexistent)?.is_none());

        Ok(())
    }

    #[test]
    fn test_cache_multiple_files() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache = SymbolCache::new(Some(temp_dir.path().to_path_buf()))?;

        // Create multiple test files
        let files: Vec<_> = (0..5)
            .map(|i| {
                let path = temp_dir.path().join(format!("test{}.rs", i));
                fs::write(&path, format!("fn func{}() {{}}", i)).unwrap();
                path
            })
            .collect();

        let symbols = vec![];

        // Cache all files
        for (i, file) in files.iter().enumerate() {
            cache.save_symbols(file, symbols.clone(), ProjectType::Rust)?;

            // Verify all previously cached files are still accessible
            for (j, file) in files.iter().enumerate().take(i + 1) {
                assert!(
                    cache.get_symbols(file, ProjectType::Rust)?.is_some(),
                    "File {} should be cached after caching file {}",
                    j,
                    i
                );
            }
        }

        // Modify one file
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&files[2], "fn func2() {}\n// modified").unwrap();

        // Only file 2 should be invalid
        for (i, file) in files.iter().enumerate() {
            let cached = cache.get_symbols(file, ProjectType::Rust)?;
            if i == 2 {
                assert!(cached.is_none(), "Modified file {} should be cache miss", i);
            } else {
                assert!(
                    cached.is_some(),
                    "Unmodified file {} should be cache hit",
                    i
                );
            }
        }

        Ok(())
    }
}
