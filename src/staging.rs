use crate::manifest::{RuntimeManifest, ACTIVE_LINK_NAME, IMAGES_DIR_NAME, MANIFEST_FILENAME};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StagingError {
    #[error("Staging failed: {0}")]
    StagingFailed(String),

    #[error("Cannot remove the active runtime. Activate a different runtime first.")]
    RemoveActiveRuntime,

    #[error("Runtime not found: {0}")]
    RuntimeNotFound(String),

    #[error("Missing extension images:\n{0}")]
    MissingImages(String),
}

#[derive(Debug)]
pub struct MissingImage {
    pub extension_name: String,
    pub expected_path: String,
}

/// Check that all extension images referenced by the manifest exist on disk.
/// Returns Ok(()) if all images are present, or Err with details of missing images.
pub fn validate_manifest_images(
    manifest: &RuntimeManifest,
    base_dir: &Path,
) -> Result<(), StagingError> {
    let missing: Vec<MissingImage> = manifest
        .extensions
        .iter()
        .filter_map(|ext| {
            let path = ext.resolve_path(base_dir);
            if path.exists() {
                None
            } else {
                Some(MissingImage {
                    extension_name: format!("{} v{}", ext.name, ext.version),
                    expected_path: path.display().to_string(),
                })
            }
        })
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        let details = missing
            .iter()
            .map(|m| format!("  {} -> {}", m.extension_name, m.expected_path))
            .collect::<Vec<_>>()
            .join("\n");
        Err(StagingError::MissingImages(details))
    }
}

/// Create the runtime directory and write the manifest file.
pub fn stage_manifest(
    manifest: &RuntimeManifest,
    manifest_json: &str,
    base_dir: &Path,
    verbose: bool,
) -> Result<(), StagingError> {
    let runtime_dir = base_dir.join("runtimes").join(&manifest.id);
    fs::create_dir_all(&runtime_dir).map_err(|e| {
        StagingError::StagingFailed(format!("Failed to create runtime directory: {e}"))
    })?;

    fs::write(runtime_dir.join(MANIFEST_FILENAME), manifest_json)
        .map_err(|e| StagingError::StagingFailed(format!("Failed to write manifest: {e}")))?;

    if verbose {
        println!(
            "  Staged runtime: {} v{} (build {})",
            manifest.runtime.name,
            manifest.runtime.version,
            &manifest.id[..8.min(manifest.id.len())]
        );
    }

    Ok(())
}

/// Copy extension images from a staging directory into the shared image pool.
/// Used by the TUF update path after downloading targets.
pub fn install_images_from_staging(
    manifest: &RuntimeManifest,
    staging_dir: &Path,
    base_dir: &Path,
    verbose: bool,
) -> Result<(), StagingError> {
    let images_dir = base_dir.join(IMAGES_DIR_NAME);
    let _ = fs::create_dir_all(&images_dir);

    for ext in &manifest.extensions {
        if let Some(ref image_id) = ext.image_id {
            let dest = images_dir.join(format!("{image_id}.raw"));
            if dest.exists() {
                if verbose {
                    println!("    Image already present: {} ({})", ext.name, image_id);
                }
                continue;
            }
            let staged_file = staging_dir.join(format!("{image_id}.raw"));
            if staged_file.exists() {
                fs::copy(&staged_file, &dest).map_err(|e| {
                    StagingError::StagingFailed(format!(
                        "Failed to install image for {}: {e}",
                        ext.name
                    ))
                })?;
                if verbose {
                    println!("    Installed image: {} -> {}.raw", ext.name, image_id);
                }
            }
        } else if let Some(ref filename) = ext.filename {
            let extensions_dir = base_dir.join("extensions");
            let _ = fs::create_dir_all(&extensions_dir);
            let staged_file = staging_dir.join(filename);
            if staged_file.exists() {
                let dest = extensions_dir.join(filename);
                if !dest.exists() || files_differ(&staged_file, &dest) {
                    fs::copy(&staged_file, &dest).map_err(|e| {
                        StagingError::StagingFailed(format!(
                            "Failed to copy extension {filename}: {e}"
                        ))
                    })?;
                    if verbose {
                        println!("    Installed extension: {filename}");
                    }
                } else if verbose {
                    println!("    Extension already up to date: {filename}");
                }
            }
        }
    }

    Ok(())
}

/// Atomically switch the active symlink to point to the given runtime.
pub fn activate_runtime(runtime_id: &str, base_dir: &Path) -> Result<(), StagingError> {
    let runtime_dir = base_dir.join("runtimes").join(runtime_id);
    if !runtime_dir.exists() {
        return Err(StagingError::RuntimeNotFound(runtime_id.to_string()));
    }

    let active_link = base_dir.join(ACTIVE_LINK_NAME);
    let active_target = format!("runtimes/{runtime_id}");

    let _ = fs::remove_file(&active_link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&active_target, &active_link).map_err(|e| {
        StagingError::StagingFailed(format!("Failed to switch active runtime: {e}"))
    })?;

    Ok(())
}

/// Remove a runtime directory. Fails if the runtime is currently active.
pub fn remove_runtime(runtime_id: &str, base_dir: &Path) -> Result<(), StagingError> {
    let active_id = resolve_active_id(base_dir);
    if active_id.as_deref() == Some(runtime_id) {
        return Err(StagingError::RemoveActiveRuntime);
    }

    let runtime_dir = base_dir.join("runtimes").join(runtime_id);
    if !runtime_dir.exists() {
        return Err(StagingError::RuntimeNotFound(runtime_id.to_string()));
    }

    fs::remove_dir_all(&runtime_dir).map_err(|e| {
        StagingError::StagingFailed(format!("Failed to remove runtime directory: {e}"))
    })?;

    Ok(())
}

fn resolve_active_id(base_dir: &Path) -> Option<String> {
    let active_path = base_dir.join(ACTIVE_LINK_NAME);
    let target = fs::read_link(&active_path).ok()?;
    target
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

fn files_differ(a: &Path, b: &Path) -> bool {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let size_a = fs::metadata(a).map(|m| m.len()).unwrap_or(0);
    let size_b = fs::metadata(b).map(|m| m.len()).unwrap_or(0);
    if size_a != size_b {
        return true;
    }

    let hash = |path: &Path| -> Option<Vec<u8>> {
        let mut file = fs::File::open(path).ok()?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = file.read(&mut buf).ok()?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Some(hasher.finalize().to_vec())
    };

    hash(a) != hash(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{ManifestExtension, RuntimeInfo};
    use std::os::unix::fs as unix_fs;
    use tempfile::TempDir;

    fn make_manifest(id: &str, name: &str, version: &str) -> RuntimeManifest {
        RuntimeManifest {
            manifest_version: 1,
            id: id.to_string(),
            built_at: "2026-02-19T00:00:00Z".to_string(),
            runtime: RuntimeInfo {
                name: name.to_string(),
                version: version.to_string(),
            },
            extensions: vec![ManifestExtension {
                name: "app".to_string(),
                version: "0.1.0".to_string(),
                filename: None,
                image_id: Some("a1b2c3d4-e5f6-5789-abcd-ef0123456789".to_string()),
            }],
        }
    }

    #[test]
    fn test_validate_manifest_images_present() {
        let tmp = TempDir::new().unwrap();
        let images_dir = tmp.path().join("images");
        fs::create_dir_all(&images_dir).unwrap();
        fs::write(
            images_dir.join("a1b2c3d4-e5f6-5789-abcd-ef0123456789.raw"),
            b"image data",
        )
        .unwrap();

        let manifest = make_manifest("test-id", "dev", "0.1.0");
        assert!(validate_manifest_images(&manifest, tmp.path()).is_ok());
    }

    #[test]
    fn test_validate_manifest_images_missing() {
        let tmp = TempDir::new().unwrap();
        let manifest = make_manifest("test-id", "dev", "0.1.0");
        let result = validate_manifest_images(&manifest, tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("app v0.1.0"));
        assert!(err.contains("a1b2c3d4-e5f6-5789-abcd-ef0123456789.raw"));
    }

    #[test]
    fn test_stage_manifest_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let manifest = make_manifest("uuid-123", "dev", "0.1.0");
        let json = serde_json::to_string_pretty(&manifest).unwrap();

        stage_manifest(&manifest, &json, tmp.path(), false).unwrap();

        let manifest_path = tmp
            .path()
            .join("runtimes")
            .join("uuid-123")
            .join("manifest.json");
        assert!(manifest_path.exists());
        let content = fs::read_to_string(manifest_path).unwrap();
        assert!(content.contains("uuid-123"));
    }

    #[test]
    fn test_activate_runtime_creates_symlink() {
        let tmp = TempDir::new().unwrap();
        let runtime_dir = tmp.path().join("runtimes").join("uuid-456");
        fs::create_dir_all(&runtime_dir).unwrap();

        activate_runtime("uuid-456", tmp.path()).unwrap();

        let active = tmp.path().join("active");
        assert!(active.exists());
        let target = fs::read_link(&active).unwrap();
        assert_eq!(target.to_str().unwrap(), "runtimes/uuid-456");
    }

    #[test]
    fn test_activate_runtime_switches_symlink() {
        let tmp = TempDir::new().unwrap();
        let rt1 = tmp.path().join("runtimes").join("uuid-1");
        let rt2 = tmp.path().join("runtimes").join("uuid-2");
        fs::create_dir_all(&rt1).unwrap();
        fs::create_dir_all(&rt2).unwrap();

        activate_runtime("uuid-1", tmp.path()).unwrap();
        activate_runtime("uuid-2", tmp.path()).unwrap();

        let target = fs::read_link(tmp.path().join("active")).unwrap();
        assert_eq!(target.to_str().unwrap(), "runtimes/uuid-2");
    }

    #[test]
    fn test_activate_runtime_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let result = activate_runtime("nonexistent", tmp.path());
        assert!(matches!(result, Err(StagingError::RuntimeNotFound(_))));
    }

    #[test]
    fn test_remove_runtime_deletes_directory() {
        let tmp = TempDir::new().unwrap();
        let runtime_dir = tmp.path().join("runtimes").join("uuid-rm");
        fs::create_dir_all(&runtime_dir).unwrap();
        fs::write(runtime_dir.join("manifest.json"), b"{}").unwrap();

        remove_runtime("uuid-rm", tmp.path()).unwrap();
        assert!(!runtime_dir.exists());
    }

    #[test]
    fn test_remove_runtime_rejects_active() {
        let tmp = TempDir::new().unwrap();
        let runtime_dir = tmp.path().join("runtimes").join("uuid-active");
        fs::create_dir_all(&runtime_dir).unwrap();
        unix_fs::symlink("runtimes/uuid-active", tmp.path().join("active")).unwrap();

        let result = remove_runtime("uuid-active", tmp.path());
        assert!(matches!(result, Err(StagingError::RemoveActiveRuntime)));
        assert!(runtime_dir.exists());
    }

    #[test]
    fn test_remove_runtime_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let result = remove_runtime("nonexistent", tmp.path());
        assert!(matches!(result, Err(StagingError::RuntimeNotFound(_))));
    }

    #[test]
    fn test_install_images_from_staging_v2() {
        let tmp = TempDir::new().unwrap();
        let staging = tmp.path().join("staging");
        fs::create_dir_all(&staging).unwrap();

        let image_id = "a1b2c3d4-e5f6-5789-abcd-ef0123456789";
        fs::write(staging.join(format!("{image_id}.raw")), b"image content").unwrap();

        let base = tmp.path().join("base");
        fs::create_dir_all(&base).unwrap();

        let manifest = make_manifest("test-id", "dev", "0.1.0");
        install_images_from_staging(&manifest, &staging, &base, false).unwrap();

        let installed = base.join("images").join(format!("{image_id}.raw"));
        assert!(installed.exists());
    }

    #[test]
    fn test_install_images_skips_existing() {
        let tmp = TempDir::new().unwrap();
        let staging = tmp.path().join("staging");
        fs::create_dir_all(&staging).unwrap();

        let image_id = "a1b2c3d4-e5f6-5789-abcd-ef0123456789";
        fs::write(staging.join(format!("{image_id}.raw")), b"new content").unwrap();

        let base = tmp.path().join("base");
        let images_dir = base.join("images");
        fs::create_dir_all(&images_dir).unwrap();
        fs::write(images_dir.join(format!("{image_id}.raw")), b"old content").unwrap();

        let manifest = make_manifest("test-id", "dev", "0.1.0");
        install_images_from_staging(&manifest, &staging, &base, false).unwrap();

        let content = fs::read_to_string(images_dir.join(format!("{image_id}.raw"))).unwrap();
        assert_eq!(content, "old content");
    }
}
