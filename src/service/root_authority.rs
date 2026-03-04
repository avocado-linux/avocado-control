use crate::config::Config;
use crate::service::error::AvocadoError;
use crate::service::types::{RootAuthorityInfo, TrustedKey};
use std::path::Path;

const METADATA_DIR_NAME: &str = "metadata";
const ROOT_JSON_FILENAME: &str = "root.json";

/// Show the trusted signing keys for this device.
pub fn show(config: &Config) -> Result<Option<RootAuthorityInfo>, AvocadoError> {
    let base_dir = config.get_avocado_base_dir();
    let root_path = Path::new(&base_dir)
        .join(METADATA_DIR_NAME)
        .join(ROOT_JSON_FILENAME);

    let content = match std::fs::read_to_string(&root_path) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    let signed_root: tough::schema::Signed<tough::schema::Root> = serde_json::from_str(&content)
        .map_err(|e| AvocadoError::ParseFailed {
            reason: format!("Failed to parse {}: {e}", root_path.display()),
        })?;

    let root = &signed_root.signed;

    let mut keys = Vec::new();
    for (key_id_decoded, key) in &root.keys {
        let key_id_hex = hex_encode(key_id_decoded.as_ref());

        let key_type = match key {
            tough::schema::key::Key::Ed25519 { .. } => "ed25519",
            tough::schema::key::Key::Rsa { .. } => "rsa",
            tough::schema::key::Key::Ecdsa { .. } | tough::schema::key::Key::EcdsaOld { .. } => {
                "ecdsa"
            }
        };

        let mut roles_for_key = Vec::new();
        for (role_type, role_keys) in &root.roles {
            let role_key_ids: Vec<String> = role_keys
                .keyids
                .iter()
                .map(|id| hex_encode(id.as_ref()))
                .collect();
            if role_key_ids.contains(&key_id_hex) {
                roles_for_key.push(role_type_display(role_type).to_string());
            }
        }

        keys.push(TrustedKey {
            key_id: key_id_hex,
            key_type: key_type.to_string(),
            roles: roles_for_key,
        });
    }

    Ok(Some(RootAuthorityInfo {
        version: root.version.get(),
        expires: root.expires.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        keys,
    }))
}

fn role_type_display(role_type: &tough::schema::RoleType) -> &'static str {
    match role_type {
        tough::schema::RoleType::Root => "authority",
        tough::schema::RoleType::Targets => "signing",
        tough::schema::RoleType::Snapshot => "metadata",
        tough::schema::RoleType::Timestamp => "freshness",
        _ => "other",
    }
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
