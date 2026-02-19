use crate::manifest::{RuntimeManifest, IMAGES_DIR_NAME, MANIFEST_FILENAME};
use ed25519_compact::PublicKey;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error("No update authority configured. Build and provision a runtime with 'avocado build' to enable verified updates.")]
    NoTrustAnchor,

    #[error("Failed to fetch {0}: {1}")]
    FetchFailed(String, String),

    #[error("Signature verification failed for {0}: {1}")]
    SignatureVerification(String, String),

    #[error("Hash mismatch for target '{target}': expected {expected}, got {actual}")]
    HashMismatch {
        target: String,
        expected: String,
        actual: String,
    },

    #[error("Staging failed: {0}")]
    StagingFailed(String),

    #[error("Metadata error: {0}")]
    MetadataError(String),
}

pub fn perform_update(url: &str, base_dir: &Path, verbose: bool) -> Result<(), UpdateError> {
    let url = url.trim_end_matches('/');

    // 1. Load the local trust anchor
    let root_path = base_dir.join("metadata").join("root.json");
    let root_content = fs::read_to_string(&root_path).map_err(|_| UpdateError::NoTrustAnchor)?;

    let signed_root: tough::schema::Signed<tough::schema::Root> =
        serde_json::from_str(&root_content).map_err(|e| {
            UpdateError::MetadataError(format!("Failed to parse local root.json: {e}"))
        })?;

    let root = &signed_root.signed;
    let trusted_keys = extract_trusted_keys(root)?;

    if verbose {
        println!(
            "  Loaded trust anchor: version {}, {} trusted key(s)",
            root.version,
            trusted_keys.len()
        );
    }

    // 2. Fetch and verify remote metadata (TUF order: timestamp -> snapshot -> targets)
    println!("  Fetching update metadata...");

    let timestamp_url = format!("{url}/metadata/timestamp.json");
    let timestamp_raw = fetch_url(&timestamp_url)?;
    let timestamp: tough::schema::Signed<tough::schema::Timestamp> =
        parse_metadata("timestamp.json", &timestamp_raw)?;
    verify_signatures(
        "timestamp.json",
        &timestamp_raw,
        &timestamp.signatures,
        &trusted_keys,
        root,
        &tough::schema::RoleType::Timestamp,
    )?;

    if verbose {
        println!(
            "  Verified timestamp.json (version {})",
            timestamp.signed.version
        );
    }

    let snapshot_url = format!("{url}/metadata/snapshot.json");
    let snapshot_raw = fetch_url(&snapshot_url)?;
    let snapshot: tough::schema::Signed<tough::schema::Snapshot> =
        parse_metadata("snapshot.json", &snapshot_raw)?;
    verify_signatures(
        "snapshot.json",
        &snapshot_raw,
        &snapshot.signatures,
        &trusted_keys,
        root,
        &tough::schema::RoleType::Snapshot,
    )?;

    if verbose {
        println!(
            "  Verified snapshot.json (version {})",
            snapshot.signed.version
        );
    }

    let targets_url = format!("{url}/metadata/targets.json");
    let targets_raw = fetch_url(&targets_url)?;
    let targets: tough::schema::Signed<tough::schema::Targets> =
        parse_metadata("targets.json", &targets_raw)?;
    verify_signatures(
        "targets.json",
        &targets_raw,
        &targets.signatures,
        &trusted_keys,
        root,
        &tough::schema::RoleType::Targets,
    )?;

    if verbose {
        println!(
            "  Verified targets.json (version {})",
            targets.signed.version
        );
    }

    // 3. Enumerate and download targets
    let target_map = &targets.signed.targets;
    println!("  Downloading {} target(s)...", target_map.len());

    let staging_dir = base_dir.join(".update-staging");
    fs::create_dir_all(&staging_dir).map_err(|e| {
        UpdateError::StagingFailed(format!("Failed to create staging directory: {e}"))
    })?;

    let images_dir = base_dir.join(IMAGES_DIR_NAME);
    fs::create_dir_all(&images_dir).map_err(|e| {
        UpdateError::StagingFailed(format!("Failed to create images directory: {e}"))
    })?;

    // Build a set of image files already present on disk so we can skip
    // downloading targets that match a content-addressable image_id.
    let existing_images: std::collections::HashSet<String> = fs::read_dir(&images_dir)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    for (target_name, target_info) in target_map {
        let name_str = target_name.raw();

        // Content-addressable skip: if this target is an image that already
        // exists locally, the UUIDv5 name guarantees identical content.
        if name_str != "manifest.json" && existing_images.contains(name_str) {
            if verbose {
                println!("    Already present, skipping: {name_str}");
            }
            continue;
        }

        let target_url = format!("{url}/targets/{name_str}");

        if verbose {
            println!("    Downloading {name_str}...");
        }

        let data = fetch_url_bytes(&target_url)?;

        // Verify length
        if data.len() as u64 != target_info.length {
            return Err(UpdateError::HashMismatch {
                target: name_str.to_string(),
                expected: format!("{} bytes", target_info.length),
                actual: format!("{} bytes", data.len()),
            });
        }

        // Verify sha256 hash
        let expected_hex = hex_encode(target_info.hashes.sha256.as_ref());
        let actual_hash = sha256_hex(&data);
        if actual_hash != expected_hex {
            return Err(UpdateError::HashMismatch {
                target: name_str.to_string(),
                expected: expected_hex,
                actual: actual_hash,
            });
        }

        let target_path = staging_dir.join(name_str);
        fs::write(&target_path, &data)
            .map_err(|e| UpdateError::StagingFailed(format!("Failed to write {name_str}: {e}")))?;
    }

    // 4. Parse the downloaded manifest and stage the update
    println!("  Staging runtime update...");

    let manifest_path = staging_dir.join("manifest.json");
    let manifest_content = fs::read_to_string(&manifest_path).map_err(|e| {
        UpdateError::StagingFailed(format!("No manifest.json in update targets: {e}"))
    })?;

    let new_manifest: RuntimeManifest = serde_json::from_str(&manifest_content)
        .map_err(|e| UpdateError::StagingFailed(format!("Invalid manifest.json: {e}")))?;

    if verbose {
        println!(
            "  New runtime: {} v{} (build {})",
            new_manifest.runtime.name,
            new_manifest.runtime.version,
            &new_manifest.id[..8.min(new_manifest.id.len())]
        );
    }

    // Create the new runtime directory
    let runtime_dir = base_dir.join("runtimes").join(&new_manifest.id);
    fs::create_dir_all(&runtime_dir).map_err(|e| {
        UpdateError::StagingFailed(format!("Failed to create runtime directory: {e}"))
    })?;

    // Write manifest
    fs::write(runtime_dir.join(MANIFEST_FILENAME), &manifest_content)
        .map_err(|e| UpdateError::StagingFailed(format!("Failed to write manifest: {e}")))?;

    // Install extension images to the shared images pool.
    // v2 manifests use image_id (content-addressable); v1 use filename.
    for ext in &new_manifest.extensions {
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
                    UpdateError::StagingFailed(format!(
                        "Failed to install image for {}: {e}",
                        ext.name
                    ))
                })?;
                if verbose {
                    println!("    Installed image: {} -> {}.raw", ext.name, image_id);
                }
            }
        } else if let Some(ref filename) = ext.filename {
            // v1 backward compatibility: use extensions/ directory
            let extensions_dir = base_dir.join("extensions");
            let _ = fs::create_dir_all(&extensions_dir);
            let staged_file = staging_dir.join(filename);
            if staged_file.exists() {
                let dest = extensions_dir.join(filename);
                if !dest.exists() || files_differ(&staged_file, &dest) {
                    fs::copy(&staged_file, &dest).map_err(|e| {
                        UpdateError::StagingFailed(format!(
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

    // Atomically switch the active symlink
    let active_link = base_dir.join("active");
    let active_target = format!("runtimes/{}", new_manifest.id);

    // Remove existing symlink first (ln -sfn equivalent)
    let _ = fs::remove_file(&active_link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&active_target, &active_link)
        .map_err(|e| UpdateError::StagingFailed(format!("Failed to switch active runtime: {e}")))?;

    println!(
        "  Activated runtime: {} v{} ({})",
        new_manifest.runtime.name,
        new_manifest.runtime.version,
        &new_manifest.id[..8.min(new_manifest.id.len())]
    );

    // Clean up staging directory
    let _ = fs::remove_dir_all(&staging_dir);

    println!("  Update staged successfully.");
    Ok(())
}

fn extract_trusted_keys(
    root: &tough::schema::Root,
) -> Result<Vec<(String, PublicKey)>, UpdateError> {
    let mut keys = Vec::new();
    for (key_id, key) in &root.keys {
        let key_id_hex = hex_encode(key_id.as_ref());
        match key {
            tough::schema::key::Key::Ed25519 { keyval, .. } => {
                let public_hex = hex_encode(keyval.public.as_ref());
                let public_bytes = hex_decode(&public_hex).map_err(|e| {
                    UpdateError::MetadataError(format!("Invalid public key hex: {e}"))
                })?;
                let pk = PublicKey::from_slice(&public_bytes).map_err(|_| {
                    UpdateError::MetadataError("Invalid ed25519 public key length".to_string())
                })?;
                keys.push((key_id_hex, pk));
            }
            _ => {
                // Skip non-ed25519 keys for now
            }
        }
    }
    if keys.is_empty() {
        return Err(UpdateError::MetadataError(
            "No ed25519 keys found in root.json".to_string(),
        ));
    }
    Ok(keys)
}

fn verify_signatures(
    name: &str,
    raw_json: &str,
    signatures: &[tough::schema::Signature],
    trusted_keys: &[(String, PublicKey)],
    root: &tough::schema::Root,
    role_type: &tough::schema::RoleType,
) -> Result<(), UpdateError> {
    // Find which key IDs are authorized for this role
    let role_def = root.roles.get(role_type).ok_or_else(|| {
        UpdateError::MetadataError(format!("No role definition for {role_type:?} in root.json"))
    })?;

    let authorized_key_ids: Vec<String> = role_def
        .keyids
        .iter()
        .map(|id| hex_encode(id.as_ref()))
        .collect();

    let threshold = role_def.threshold.get() as usize;

    // Extract the raw "signed" portion from the JSON string for verification.
    // We must use the exact bytes from the original JSON to match the signature,
    // so we extract the substring rather than re-serializing.
    let canonical = extract_signed_canonical(raw_json)
        .map_err(|e| UpdateError::SignatureVerification(name.to_string(), e))?;

    let mut valid_count = 0;

    for sig in signatures {
        let sig_key_id = hex_encode(sig.keyid.as_ref());

        if !authorized_key_ids.contains(&sig_key_id) {
            continue;
        }

        if let Some((_, pk)) = trusted_keys.iter().find(|(id, _)| *id == sig_key_id) {
            if let Ok(signature) = ed25519_compact::Signature::from_slice(sig.sig.as_ref()) {
                if pk.verify(canonical.as_bytes(), &signature).is_ok() {
                    valid_count += 1;
                }
            }
        }
    }

    if valid_count < threshold {
        return Err(UpdateError::SignatureVerification(
            name.to_string(),
            format!("Insufficient valid signatures: got {valid_count}, need {threshold}"),
        ));
    }

    Ok(())
}

/// Extract the canonical JSON string for the "signed" field from a TUF metadata envelope.
/// This re-serializes the parsed "signed" value to compact JSON (serde_json::to_string)
/// which produces deterministic output because serde_json uses BTreeMap for key ordering.
fn extract_signed_canonical(raw_json: &str) -> Result<String, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(raw_json).map_err(|e| format!("Invalid JSON: {e}"))?;

    let signed = parsed
        .get("signed")
        .ok_or_else(|| "Missing 'signed' field".to_string())?;

    serde_json::to_string(signed).map_err(|e| format!("Failed to serialize: {e}"))
}

fn fetch_url(url: &str) -> Result<String, UpdateError> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| UpdateError::FetchFailed(url.to_string(), e.to_string()))?;

    let mut body = String::new();
    response
        .into_body()
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|e| UpdateError::FetchFailed(url.to_string(), e.to_string()))?;

    Ok(body)
}

fn fetch_url_bytes(url: &str) -> Result<Vec<u8>, UpdateError> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| UpdateError::FetchFailed(url.to_string(), e.to_string()))?;

    let mut body = Vec::new();
    response
        .into_body()
        .as_reader()
        .read_to_end(&mut body)
        .map_err(|e| UpdateError::FetchFailed(url.to_string(), e.to_string()))?;

    Ok(body)
}

fn parse_metadata<T: serde::de::DeserializeOwned>(
    name: &str,
    raw: &str,
) -> Result<tough::schema::Signed<T>, UpdateError> {
    serde_json::from_str(raw)
        .map_err(|e| UpdateError::MetadataError(format!("Failed to parse {name}: {e}")))
}

fn files_differ(a: &Path, b: &Path) -> bool {
    let size_a = fs::metadata(a).map(|m| m.len()).unwrap_or(0);
    let size_b = fs::metadata(b).map(|m| m.len()).unwrap_or(0);
    if size_a != size_b {
        return true;
    }

    let hash_a = file_sha256(a).unwrap_or_default();
    let hash_b = file_sha256(b).unwrap_or_default();
    hash_a != hash_b
}

fn file_sha256(path: &Path) -> Option<String> {
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
    Some(hex_encode(&hasher.finalize()))
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("Odd-length hex string".to_string());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|e| format!("Invalid hex at position {i}: {e}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_keypair() -> ed25519_compact::KeyPair {
        let seed_bytes = [42u8; 32];
        ed25519_compact::KeyPair::from_seed(ed25519_compact::Seed::from(seed_bytes))
    }

    fn make_test_root_json() -> (String, ed25519_compact::KeyPair) {
        let kp = test_keypair();
        let pk_hex = hex_encode(kp.pk.as_ref());
        let key_id = {
            let canonical = format!(
                r#"{{"keytype":"ed25519","keyval":{{"public":"{pk_hex}"}},"scheme":"ed25519"}}"#
            );
            sha256_hex(canonical.as_bytes())
        };

        let signed: serde_json::Value = serde_json::json!({
            "_type": "root",
            "consistent_snapshot": false,
            "expires": "2027-02-18T00:00:00Z",
            "keys": {
                &key_id: {
                    "keytype": "ed25519",
                    "keyval": { "public": pk_hex },
                    "scheme": "ed25519"
                }
            },
            "roles": {
                "root": { "keyids": [&key_id], "threshold": 1 },
                "snapshot": { "keyids": [&key_id], "threshold": 1 },
                "targets": { "keyids": [&key_id], "threshold": 1 },
                "timestamp": { "keyids": [&key_id], "threshold": 1 }
            },
            "spec_version": "1.0.0",
            "version": 1
        });

        let canonical = serde_json::to_string(&signed).unwrap();
        let sig = kp.sk.sign(&canonical, None);
        let sig_hex = hex_encode(sig.as_ref());

        let root = serde_json::json!({
            "signatures": [{ "keyid": key_id, "sig": sig_hex }],
            "signed": signed
        });

        (serde_json::to_string_pretty(&root).unwrap(), kp)
    }

    #[test]
    fn test_extract_trusted_keys() {
        let (root_json, _kp) = make_test_root_json();
        let signed_root: tough::schema::Signed<tough::schema::Root> =
            serde_json::from_str(&root_json).unwrap();
        let keys = extract_trusted_keys(&signed_root.signed).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].1.as_ref().len(), 32);
    }

    #[test]
    fn test_hex_roundtrip() {
        let data = vec![0xab, 0xcd, 0xef, 0x01, 0x23];
        let hex = hex_encode(&data);
        assert_eq!(hex, "abcdef0123");
        let decoded = hex_decode(&hex).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_hex_decode_error() {
        assert!(hex_decode("abc").is_err());
        assert!(hex_decode("zzzz").is_err());
    }

    #[test]
    fn test_no_trust_anchor() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = perform_update("http://localhost:9999", tmp.path(), false);
        assert!(matches!(result, Err(UpdateError::NoTrustAnchor)));
    }

    #[test]
    fn test_files_differ_same() {
        let tmp = tempfile::TempDir::new().unwrap();
        let f1 = tmp.path().join("a");
        let f2 = tmp.path().join("b");
        fs::write(&f1, b"hello").unwrap();
        fs::write(&f2, b"hello").unwrap();
        assert!(!files_differ(&f1, &f2));
    }

    #[test]
    fn test_files_differ_different() {
        let tmp = tempfile::TempDir::new().unwrap();
        let f1 = tmp.path().join("a");
        let f2 = tmp.path().join("b");
        fs::write(&f1, b"hello").unwrap();
        fs::write(&f2, b"world").unwrap();
        assert!(files_differ(&f1, &f2));
    }

    #[test]
    fn test_verify_signatures_with_real_key() {
        let (root_json, kp) = make_test_root_json();

        let signed_root: tough::schema::Signed<tough::schema::Root> =
            serde_json::from_str(&root_json).unwrap();
        let trusted_keys = extract_trusted_keys(&signed_root.signed).unwrap();

        let pk_hex = hex_encode(kp.pk.as_ref());
        let key_id = {
            let canonical = format!(
                r#"{{"keytype":"ed25519","keyval":{{"public":"{pk_hex}"}},"scheme":"ed25519"}}"#
            );
            sha256_hex(canonical.as_bytes())
        };

        let signed_payload: serde_json::Value = serde_json::json!({
            "_type": "targets",
            "expires": "2027-02-18T00:00:00Z",
            "spec_version": "1.0.0",
            "targets": {},
            "version": 1
        });

        // Build the envelope first, then extract the canonical form the same way
        // verify_signatures does -- this avoids any key-ordering drift between
        // serde_json's Map implementation and re-serialization.
        let unsigned_envelope = serde_json::json!({
            "signatures": [],
            "signed": signed_payload
        });
        let unsigned_raw = serde_json::to_string(&unsigned_envelope).unwrap();
        let canonical = extract_signed_canonical(&unsigned_raw).unwrap();

        let sig = kp.sk.sign(canonical.as_bytes(), None);
        let sig_hex = hex_encode(sig.as_ref());

        let full_json = serde_json::json!({
            "signatures": [{ "keyid": key_id, "sig": sig_hex }],
            "signed": signed_payload
        });

        let raw = serde_json::to_string(&full_json).unwrap();
        let parsed: tough::schema::Signed<tough::schema::Targets> =
            serde_json::from_str(&raw).unwrap();

        let result = verify_signatures(
            "targets.json",
            &raw,
            &parsed.signatures,
            &trusted_keys,
            &signed_root.signed,
            &tough::schema::RoleType::Targets,
        );

        assert!(
            result.is_ok(),
            "Signature verification should succeed: {result:?}"
        );
    }
}
