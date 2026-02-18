use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const DEFAULT_AVOCADO_DIR: &str = "/var/lib/avocado";
pub const ACTIVE_LINK_NAME: &str = "active";
pub const RUNTIMES_DIR_NAME: &str = "runtimes";
pub const MANIFEST_FILENAME: &str = "manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeManifest {
    pub manifest_version: u32,
    pub id: String,
    pub built_at: String,
    pub runtime: RuntimeInfo,
    pub extensions: Vec<ManifestExtension>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestExtension {
    pub name: String,
    pub version: String,
    pub filename: String,
}

impl RuntimeManifest {
    /// Resolve the avocado base directory, checking env override for testing.
    pub fn base_dir() -> String {
        std::env::var("AVOCADO_BASE_DIR").unwrap_or_else(|_| DEFAULT_AVOCADO_DIR.to_string())
    }

    /// Load a manifest from a specific directory containing manifest.json.
    pub fn load_from(dir: &Path) -> Option<Self> {
        let manifest_path = dir.join(MANIFEST_FILENAME);
        let content = fs::read_to_string(&manifest_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Load the active runtime manifest by following the `active` symlink.
    /// Returns None if no active symlink or manifest file exists.
    pub fn load_active(base_dir: &Path) -> Option<Self> {
        let active_path = base_dir.join(ACTIVE_LINK_NAME);
        if !active_path.exists() {
            return None;
        }
        Self::load_from(&active_path)
    }

    /// Resolve the UUID directory name that the `active` symlink points to.
    fn resolve_active_id(base_dir: &Path) -> Option<String> {
        let active_path = base_dir.join(ACTIVE_LINK_NAME);
        let target = fs::read_link(&active_path).ok()?;
        // target is relative like "runtimes/<uuid>" -- extract the last component
        target
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
    }

    /// List all available runtime manifests.
    /// Returns each manifest paired with a bool indicating whether it is the active runtime.
    /// Sorted by (name ASC, version ASC, built_at DESC).
    pub fn list_all(base_dir: &Path) -> Vec<(Self, bool)> {
        let active_id = Self::resolve_active_id(base_dir);
        let runtimes_dir = base_dir.join(RUNTIMES_DIR_NAME);

        let mut results: Vec<(Self, bool)> = Vec::new();

        let entries = match fs::read_dir(&runtimes_dir) {
            Ok(entries) => entries,
            Err(_) => return results,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Some(manifest) = Self::load_from(&path) {
                let dir_name = entry
                    .file_name()
                    .to_str()
                    .unwrap_or_default()
                    .to_string();
                let is_active = active_id.as_deref() == Some(&dir_name);
                results.push((manifest, is_active));
            }
        }

        results.sort_by(|(a, _), (b, _)| {
            a.runtime
                .name
                .cmp(&b.runtime.name)
                .then_with(|| a.runtime.version.cmp(&b.runtime.version))
                .then_with(|| b.built_at.cmp(&a.built_at)) // newest first
        });

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs as unix_fs;
    use tempfile::TempDir;

    fn write_manifest(dir: &Path, manifest: &RuntimeManifest) {
        fs::create_dir_all(dir).unwrap();
        let json = serde_json::to_string_pretty(manifest).unwrap();
        fs::write(dir.join(MANIFEST_FILENAME), json).unwrap();
    }

    fn make_manifest(id: &str, name: &str, version: &str, built_at: &str) -> RuntimeManifest {
        RuntimeManifest {
            manifest_version: 1,
            id: id.to_string(),
            built_at: built_at.to_string(),
            runtime: RuntimeInfo {
                name: name.to_string(),
                version: version.to_string(),
            },
            extensions: vec![ManifestExtension {
                name: "app".to_string(),
                version: "0.1.0".to_string(),
                filename: "app-0.1.0.raw".to_string(),
            }],
        }
    }

    #[test]
    fn test_load_from() {
        let tmp = TempDir::new().unwrap();
        let rt_dir = tmp.path().join("runtimes").join("abc-123");
        let manifest = make_manifest("abc-123", "dev", "0.1.0", "2026-02-18T15:00:00Z");
        write_manifest(&rt_dir, &manifest);

        let loaded = RuntimeManifest::load_from(&rt_dir).unwrap();
        assert_eq!(loaded.id, "abc-123");
        assert_eq!(loaded.runtime.name, "dev");
        assert_eq!(loaded.runtime.version, "0.1.0");
        assert_eq!(loaded.extensions.len(), 1);
    }

    #[test]
    fn test_load_from_missing() {
        let tmp = TempDir::new().unwrap();
        assert!(RuntimeManifest::load_from(tmp.path()).is_none());
    }

    #[test]
    fn test_load_active() {
        let tmp = TempDir::new().unwrap();
        let rt_dir = tmp.path().join("runtimes").join("uuid-1");
        let manifest = make_manifest("uuid-1", "dev", "0.1.0", "2026-02-18T15:00:00Z");
        write_manifest(&rt_dir, &manifest);

        unix_fs::symlink("runtimes/uuid-1", tmp.path().join("active")).unwrap();

        let loaded = RuntimeManifest::load_active(tmp.path()).unwrap();
        assert_eq!(loaded.id, "uuid-1");
    }

    #[test]
    fn test_load_active_missing_symlink() {
        let tmp = TempDir::new().unwrap();
        assert!(RuntimeManifest::load_active(tmp.path()).is_none());
    }

    #[test]
    fn test_list_all_sorted() {
        let tmp = TempDir::new().unwrap();
        let runtimes = tmp.path().join("runtimes");

        let m1 = make_manifest("aaa", "dev", "0.1.0", "2026-02-17T10:00:00Z");
        let m2 = make_manifest("bbb", "dev", "0.1.0", "2026-02-18T15:00:00Z");
        let m3 = make_manifest("ccc", "dev", "0.2.0", "2026-02-16T09:00:00Z");

        write_manifest(&runtimes.join("aaa"), &m1);
        write_manifest(&runtimes.join("bbb"), &m2);
        write_manifest(&runtimes.join("ccc"), &m3);

        unix_fs::symlink("runtimes/bbb", tmp.path().join("active")).unwrap();

        let list = RuntimeManifest::list_all(tmp.path());
        assert_eq!(list.len(), 3);

        // Same name+version group: newest first
        assert_eq!(list[0].0.id, "bbb");
        assert!(list[0].1); // active
        assert_eq!(list[1].0.id, "aaa");
        assert!(!list[1].1);
        // Different version
        assert_eq!(list[2].0.id, "ccc");
        assert!(!list[2].1);
    }

    #[test]
    fn test_list_all_no_runtimes_dir() {
        let tmp = TempDir::new().unwrap();
        let list = RuntimeManifest::list_all(tmp.path());
        assert!(list.is_empty());
    }

    #[test]
    fn test_manifest_serialization_roundtrip() {
        let manifest = make_manifest("test-id", "prod", "1.2.3", "2026-01-01T00:00:00Z");
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: RuntimeManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "test-id");
        assert_eq!(parsed.runtime.name, "prod");
        assert_eq!(parsed.runtime.version, "1.2.3");
        assert_eq!(parsed.built_at, "2026-01-01T00:00:00Z");
    }
}
