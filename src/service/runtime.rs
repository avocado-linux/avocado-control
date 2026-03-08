use crate::config::Config;
use crate::manifest::RuntimeManifest;
use crate::service::error::AvocadoError;
use crate::service::types::{RuntimeEntry, RuntimeExtensionInfo};
use crate::{staging, update};
use std::path::Path;
use std::sync::mpsc;
use std::thread;

/// A streaming operation: a channel receiver for log messages and a join handle for the result.
type StreamHandle = (
    mpsc::Receiver<String>,
    thread::JoinHandle<Result<(), AvocadoError>>,
);

/// Convert a RuntimeManifest + active flag to a RuntimeEntry.
fn manifest_to_entry(manifest: &RuntimeManifest, active: bool) -> RuntimeEntry {
    RuntimeEntry {
        id: manifest.id.clone(),
        manifest_version: manifest.manifest_version,
        built_at: manifest.built_at.clone(),
        name: manifest.runtime.name.clone(),
        version: manifest.runtime.version.clone(),
        extensions: manifest
            .extensions
            .iter()
            .map(|e| RuntimeExtensionInfo {
                name: e.name.clone(),
                version: e.version.clone(),
                image_id: e.image_id.clone(),
            })
            .collect(),
        active,
    }
}

/// List all available runtimes.
pub fn list_runtimes(config: &Config) -> Result<Vec<RuntimeEntry>, AvocadoError> {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);
    let runtimes = RuntimeManifest::list_all(base_path);

    Ok(runtimes
        .iter()
        .map(|(m, active)| manifest_to_entry(m, *active))
        .collect())
}

// ── Streaming service functions ──────────────────────────────────────────────

/// Add a runtime from a TUF repository URL with streaming output.
/// Performs the TUF update synchronously, then streams the refresh operation.
pub fn add_from_url_streaming(
    url: &str,
    auth_token: Option<&str>,
    config: &Config,
) -> Result<StreamHandle, AvocadoError> {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);
    update::perform_update(url, base_path, auth_token, false)?;
    Ok(super::ext::refresh_extensions_streaming(config))
}

/// Add a runtime from a local manifest file with streaming output.
/// Performs staging synchronously, then streams the refresh operation.
pub fn add_from_manifest_streaming(
    manifest_path: &str,
    config: &Config,
) -> Result<StreamHandle, AvocadoError> {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);

    let manifest_content =
        std::fs::read_to_string(manifest_path).map_err(|e| AvocadoError::StagingFailed {
            reason: format!("Failed to read manifest: {e}"),
        })?;

    let manifest: RuntimeManifest =
        serde_json::from_str(&manifest_content).map_err(|e| AvocadoError::StagingFailed {
            reason: format!("Invalid manifest.json: {e}"),
        })?;

    staging::validate_manifest_images(&manifest, base_path)?;
    staging::stage_manifest(&manifest, &manifest_content, base_path, false)?;
    staging::activate_runtime(&manifest.id, base_path)?;
    Ok(super::ext::refresh_extensions_streaming(config))
}

/// Activate a staged runtime by ID (or prefix) with streaming output.
/// Performs activation synchronously, then streams the refresh operation.
pub fn activate_runtime_streaming(
    id_prefix: &str,
    config: &Config,
) -> Result<Option<StreamHandle>, AvocadoError> {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);
    let runtimes = RuntimeManifest::list_all(base_path);

    let (matched, is_active) = resolve_runtime_with_active(id_prefix, &runtimes)?;
    if is_active {
        return Ok(None); // Already active, nothing to do
    }

    staging::activate_runtime(&matched.id, base_path)?;
    Ok(Some(super::ext::refresh_extensions_streaming(config)))
}

// ── Batch service functions ──────────────────────────────────────────────────

/// Add a runtime from a TUF repository URL.
/// Returns log messages from the refresh operation.
pub fn add_from_url(
    url: &str,
    auth_token: Option<&str>,
    config: &Config,
) -> Result<Vec<String>, AvocadoError> {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);
    update::perform_update(url, base_path, auth_token, false)?;
    super::ext::refresh_extensions(config)
}

/// Add a runtime from a local manifest file.
/// Returns log messages from the refresh operation.
pub fn add_from_manifest(
    manifest_path: &str,
    config: &Config,
) -> Result<Vec<String>, AvocadoError> {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);

    let manifest_content =
        std::fs::read_to_string(manifest_path).map_err(|e| AvocadoError::StagingFailed {
            reason: format!("Failed to read manifest: {e}"),
        })?;

    let manifest: RuntimeManifest =
        serde_json::from_str(&manifest_content).map_err(|e| AvocadoError::StagingFailed {
            reason: format!("Invalid manifest.json: {e}"),
        })?;

    staging::validate_manifest_images(&manifest, base_path)?;
    staging::stage_manifest(&manifest, &manifest_content, base_path, false)?;
    staging::activate_runtime(&manifest.id, base_path)?;
    super::ext::refresh_extensions(config)
}

/// Remove a staged runtime by ID (or prefix).
pub fn remove_runtime(id_prefix: &str, config: &Config) -> Result<(), AvocadoError> {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);
    let runtimes = RuntimeManifest::list_all(base_path);

    let matched = resolve_runtime(id_prefix, &runtimes)?;
    staging::remove_runtime(&matched.id, base_path)?;
    Ok(())
}

/// Activate a staged runtime by ID (or prefix).
/// Returns log messages from the refresh operation.
pub fn activate_runtime(id_prefix: &str, config: &Config) -> Result<Vec<String>, AvocadoError> {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);
    let runtimes = RuntimeManifest::list_all(base_path);

    let (matched, is_active) = resolve_runtime_with_active(id_prefix, &runtimes)?;
    if is_active {
        return Ok(Vec::new()); // Already active, nothing to do
    }

    staging::activate_runtime(&matched.id, base_path)?;
    super::ext::refresh_extensions(config)
}

/// Inspect a runtime's details by ID (or prefix).
pub fn inspect_runtime(id_prefix: &str, config: &Config) -> Result<RuntimeEntry, AvocadoError> {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);
    let runtimes = RuntimeManifest::list_all(base_path);

    let (matched, is_active) = resolve_runtime_with_active(id_prefix, &runtimes)?;
    Ok(manifest_to_entry(matched, is_active))
}

/// Resolve a runtime ID prefix to a unique RuntimeManifest.
fn resolve_runtime<'a>(
    id_prefix: &str,
    runtimes: &'a [(RuntimeManifest, bool)],
) -> Result<&'a RuntimeManifest, AvocadoError> {
    let (matched, _) = resolve_runtime_with_active(id_prefix, runtimes)?;
    Ok(matched)
}

/// Resolve a runtime ID prefix, returning the manifest and its active status.
fn resolve_runtime_with_active<'a>(
    id_prefix: &str,
    runtimes: &'a [(RuntimeManifest, bool)],
) -> Result<(&'a RuntimeManifest, bool), AvocadoError> {
    let matches: Vec<&(RuntimeManifest, bool)> = runtimes
        .iter()
        .filter(|(m, _)| m.id.starts_with(id_prefix))
        .collect();

    match matches.len() {
        0 => Err(AvocadoError::RuntimeNotFound {
            id: id_prefix.to_string(),
        }),
        1 => Ok((&matches[0].0, matches[0].1)),
        _ => {
            let candidates: Vec<String> = matches
                .iter()
                .map(|(m, _)| m.id[..8.min(m.id.len())].to_string())
                .collect();
            Err(AvocadoError::AmbiguousRuntimeId {
                id: id_prefix.to_string(),
                candidates,
            })
        }
    }
}
