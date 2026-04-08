use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::manifest::METADATA_FILENAME;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetadata {
    pub version: u32,
    pub entries: HashMap<String, String>,
}

impl RuntimeMetadata {
    /// Create an empty metadata store.
    pub fn new() -> Self {
        Self {
            version: 1,
            entries: HashMap::new(),
        }
    }

    /// Load metadata from a runtime directory. Returns empty metadata if the file
    /// is missing or corrupt (graceful degradation).
    pub fn load(runtime_dir: &Path) -> Self {
        let path = runtime_dir.join(METADATA_FILENAME);
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| Self::new()),
            Err(_) => Self::new(),
        }
    }

    /// Save metadata to a runtime directory.
    pub fn save(&self, runtime_dir: &Path) -> Result<(), std::io::Error> {
        let path = runtime_dir.join(METADATA_FILENAME);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(&path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_is_empty() {
        let meta = RuntimeMetadata::new();
        assert_eq!(meta.version, 1);
        assert!(meta.entries.is_empty());
    }

    #[test]
    fn test_load_missing_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let meta = RuntimeMetadata::load(tmp.path());
        assert_eq!(meta.version, 1);
        assert!(meta.entries.is_empty());
    }

    #[test]
    fn test_load_corrupt_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(METADATA_FILENAME), "not json").unwrap();
        let meta = RuntimeMetadata::load(tmp.path());
        assert_eq!(meta.version, 1);
        assert!(meta.entries.is_empty());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut meta = RuntimeMetadata::new();
        meta.entries.insert("foo".to_string(), "bar".to_string());
        meta.entries
            .insert("environment".to_string(), "staging".to_string());

        meta.save(tmp.path()).unwrap();
        let loaded = RuntimeMetadata::load(tmp.path());
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries.get("foo").unwrap(), "bar");
        assert_eq!(loaded.entries.get("environment").unwrap(), "staging");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut meta = RuntimeMetadata::new();
        meta.entries.insert("key".to_string(), "value".to_string());

        let json = serde_json::to_string(&meta).unwrap();
        let parsed: RuntimeMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.entries.get("key").unwrap(), "value");
    }

    #[test]
    fn test_overwrite_existing() {
        let tmp = TempDir::new().unwrap();
        let mut meta = RuntimeMetadata::new();
        meta.entries.insert("k".to_string(), "v1".to_string());
        meta.save(tmp.path()).unwrap();

        let mut meta2 = RuntimeMetadata::load(tmp.path());
        meta2.entries.insert("k".to_string(), "v2".to_string());
        meta2.save(tmp.path()).unwrap();

        let loaded = RuntimeMetadata::load(tmp.path());
        assert_eq!(loaded.entries.get("k").unwrap(), "v2");
    }
}
