use crate::config::Config;
use crate::manifest::RuntimeManifest;
use crate::output::OutputManager;
use crate::update;
use clap::{Arg, ArgMatches, Command};
use std::path::Path;

pub fn create_command() -> Command {
    Command::new("runtime")
        .about("Manage runtimes")
        .subcommand(Command::new("list").about("List available runtimes"))
        .subcommand(
            Command::new("update-authority")
                .about("Show update authority (trusted signing keys) for this device"),
        )
        .subcommand(
            Command::new("update")
                .about("Check for and apply runtime updates from a TUF repository")
                .arg(
                    Arg::new("url")
                        .long("url")
                        .required(true)
                        .help("URL of the TUF update repository"),
                ),
        )
}

pub fn handle_command(matches: &ArgMatches, config: &Config, output: &OutputManager) {
    match matches.subcommand() {
        Some(("list", _)) => {
            list_runtimes(config, output);
        }
        Some(("update-authority", _)) => {
            show_update_authority(config, output);
        }
        Some(("update", update_matches)) => {
            handle_update(update_matches, config, output);
        }
        _ => {
            println!("Use 'runtime list' to see available runtimes.");
            println!("Run 'avocadoctl runtime --help' for more information.");
        }
    }
}

fn handle_update(matches: &ArgMatches, config: &Config, output: &OutputManager) {
    let url = matches.get_one::<String>("url").expect("url is required");

    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);

    println!();
    println!("  Checking for runtime update from {url}");
    println!();

    match update::perform_update(url, base_path, output.is_verbose()) {
        Ok(()) => {
            println!();
            output.success("Runtime Update", "Update applied successfully.");
        }
        Err(e) => {
            println!();
            output.error("Runtime Update", &format!("{e}"));
            std::process::exit(1);
        }
    }
}

const METADATA_DIR_NAME: &str = "metadata";
const ROOT_JSON_FILENAME: &str = "root.json";

fn show_update_authority(config: &Config, output: &OutputManager) {
    let base_dir = config.get_avocado_base_dir();
    let root_path = Path::new(&base_dir)
        .join(METADATA_DIR_NAME)
        .join(ROOT_JSON_FILENAME);

    let content = match std::fs::read_to_string(&root_path) {
        Ok(c) => c,
        Err(_) => {
            output.info(
                "Update Authority",
                "No update authority configured. Build and provision a runtime with avocado build to enable verified updates.",
            );
            return;
        }
    };

    let signed_root: tough::schema::Signed<tough::schema::Root> =
        match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(e) => {
                output.error(
                    "Update Authority",
                    &format!("Failed to parse {}: {e}", root_path.display()),
                );
                return;
            }
        };

    let root = &signed_root.signed;

    println!();
    println!("  Update authority:");
    println!();
    println!("    Version:  {}", root.version);
    println!(
        "    Expires:  {}",
        root.expires.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!();

    println!("    Trusted signing keys:");
    println!();
    println!("      {:<18} {:<12} ROLES", "KEY ID", "TYPE");

    for (key_id_decoded, key) in &root.keys {
        let key_id_hex = hex_encode(key_id_decoded.as_ref());
        let short_id = &key_id_hex[..std::cmp::min(16, key_id_hex.len())];

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
                roles_for_key.push(role_type_display(role_type));
            }
        }

        let all_roles = ["signing", "authority", "metadata", "freshness"];
        let roles_str = if roles_for_key.len() == all_roles.len() {
            "all".to_string()
        } else {
            roles_for_key.join(", ")
        };

        println!("      {short_id:<18} {key_type:<12} {roles_str}");
    }

    println!();

    if output.is_verbose() {
        println!("    Full key IDs:");
        for key_id_decoded in root.keys.keys() {
            println!("      {}", hex_encode(key_id_decoded.as_ref()));
        }
        println!();
        println!("    Metadata path: {}", root_path.display());
        println!();
    }
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

#[cfg(test)]
fn parse_root_json(content: &str) -> Result<tough::schema::Signed<tough::schema::Root>, String> {
    serde_json::from_str(content).map_err(|e| format!("Failed to parse root.json: {e}"))
}

fn list_runtimes(config: &Config, output: &OutputManager) {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);

    let runtimes = RuntimeManifest::list_all(base_path);

    if runtimes.is_empty() {
        output.info(
            "Runtime List",
            "No runtimes found. Build and provision a runtime first.",
        );
        return;
    }

    println!();
    println!(
        "  {:<16} {:<12} {:<10} {:<24} STATUS",
        "NAME", "VERSION", "BUILD ID", "BUILT AT"
    );

    for (manifest, is_active) in &runtimes {
        let short_id = if manifest.id.len() >= 8 {
            &manifest.id[..8]
        } else {
            &manifest.id
        };

        let built_at_display = manifest.built_at.replace('T', " ").replace('Z', "");

        let status = if *is_active { "active" } else { "" };

        println!(
            "  {:<16} {:<12} {:<10} {:<24} {}",
            manifest.runtime.name, manifest.runtime.version, short_id, built_at_display, status
        );
    }

    println!();

    if output.is_verbose() {
        println!("  Full build IDs:");
        for (manifest, is_active) in &runtimes {
            let marker = if *is_active { " (active)" } else { "" };
            println!("    {} {}{marker}", manifest.id, manifest.runtime.name,);
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root.json generated by avocado-cli's update_signing module (ed25519, all roles, version 1).
    fn sample_root_json() -> &'static str {
        r#"{
  "signatures": [
    {
      "keyid": "47d8c89a68ff5a42a3810a50a9223689604657e75f603b84e21c6dc5de49533d",
      "sig": "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
    }
  ],
  "signed": {
    "_type": "root",
    "consistent_snapshot": false,
    "expires": "2027-02-18T00:00:00Z",
    "keys": {
      "47d8c89a68ff5a42a3810a50a9223689604657e75f603b84e21c6dc5de49533d": {
        "keytype": "ed25519",
        "keyval": {
          "public": "a4b3c2d1e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7"
        },
        "scheme": "ed25519"
      }
    },
    "roles": {
      "root": {
        "keyids": ["47d8c89a68ff5a42a3810a50a9223689604657e75f603b84e21c6dc5de49533d"],
        "threshold": 1
      },
      "snapshot": {
        "keyids": ["47d8c89a68ff5a42a3810a50a9223689604657e75f603b84e21c6dc5de49533d"],
        "threshold": 1
      },
      "targets": {
        "keyids": ["47d8c89a68ff5a42a3810a50a9223689604657e75f603b84e21c6dc5de49533d"],
        "threshold": 1
      },
      "timestamp": {
        "keyids": ["47d8c89a68ff5a42a3810a50a9223689604657e75f603b84e21c6dc5de49533d"],
        "threshold": 1
      }
    },
    "spec_version": "1.0.0",
    "version": 1
  }
}"#
    }

    #[test]
    fn test_parse_root_json_valid() {
        let result = parse_root_json(sample_root_json());
        assert!(
            result.is_ok(),
            "Failed to parse valid root.json: {:?}",
            result.err()
        );

        let signed_root = result.unwrap();
        let root = &signed_root.signed;

        assert_eq!(root.version.get(), 1);
        assert_eq!(root.spec_version, "1.0.0");
        assert!(!root.consistent_snapshot);
    }

    #[test]
    fn test_parse_root_json_has_all_roles() {
        let signed_root = parse_root_json(sample_root_json()).unwrap();
        let root = &signed_root.signed;

        assert!(root.roles.contains_key(&tough::schema::RoleType::Root));
        assert!(root.roles.contains_key(&tough::schema::RoleType::Targets));
        assert!(root.roles.contains_key(&tough::schema::RoleType::Snapshot));
        assert!(root.roles.contains_key(&tough::schema::RoleType::Timestamp));
    }

    #[test]
    fn test_parse_root_json_key_info() {
        let signed_root = parse_root_json(sample_root_json()).unwrap();
        let root = &signed_root.signed;

        assert_eq!(root.keys.len(), 1);

        for (_, key) in &root.keys {
            assert!(matches!(key, tough::schema::key::Key::Ed25519 { .. }));
        }
    }

    #[test]
    fn test_parse_root_json_thresholds() {
        let signed_root = parse_root_json(sample_root_json()).unwrap();
        let root = &signed_root.signed;

        for (_, role_keys) in &root.roles {
            assert_eq!(role_keys.threshold.get(), 1);
        }
    }

    #[test]
    fn test_role_type_display_mapping() {
        assert_eq!(
            role_type_display(&tough::schema::RoleType::Root),
            "authority"
        );
        assert_eq!(
            role_type_display(&tough::schema::RoleType::Targets),
            "signing"
        );
        assert_eq!(
            role_type_display(&tough::schema::RoleType::Snapshot),
            "metadata"
        );
        assert_eq!(
            role_type_display(&tough::schema::RoleType::Timestamp),
            "freshness"
        );
    }

    #[test]
    fn test_parse_root_json_invalid() {
        let result = parse_root_json("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0xab, 0xcd, 0xef]), "abcdef");
        assert_eq!(hex_encode(&[0x00, 0xff]), "00ff");
        assert_eq!(hex_encode(&[]), "");
    }

    #[test]
    fn test_parse_root_json_signature_present() {
        let signed_root = parse_root_json(sample_root_json()).unwrap();
        assert_eq!(signed_root.signatures.len(), 1);
    }
}
