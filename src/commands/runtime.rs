use crate::config::Config;
use crate::manifest::RuntimeManifest;
use crate::output::OutputManager;
use crate::{staging, update};
use clap::{Arg, ArgGroup, ArgMatches, Command};
use std::path::Path;

pub fn create_command() -> Command {
    Command::new("runtime")
        .about("Manage runtimes")
        .subcommand(Command::new("list").about("List available runtimes"))
        .subcommand(
            Command::new("add")
                .about("Add a runtime from a TUF repository or local manifest")
                .arg(
                    Arg::new("url")
                        .long("url")
                        .help("URL of a TUF update repository"),
                )
                .arg(
                    Arg::new("manifest")
                        .long("manifest")
                        .help("Path to a local manifest.json file"),
                )
                .group(
                    ArgGroup::new("source")
                        .args(["url", "manifest"])
                        .required(true),
                ),
        )
        .subcommand(
            Command::new("remove").about("Remove a staged runtime").arg(
                Arg::new("id")
                    .required(true)
                    .help("Runtime build ID (full or prefix)"),
            ),
        )
        .subcommand(
            Command::new("activate")
                .about("Activate a staged runtime")
                .arg(
                    Arg::new("id")
                        .required(true)
                        .help("Runtime build ID (full or prefix)"),
                ),
        )
        .subcommand(
            Command::new("inspect")
                .about("Inspect a runtime's details and extensions")
                .arg(
                    Arg::new("id")
                        .required(true)
                        .help("Runtime build ID (full or prefix)"),
                ),
        )
}

pub fn handle_command(matches: &ArgMatches, config: &Config, output: &OutputManager) {
    match matches.subcommand() {
        Some(("list", _)) => {
            list_runtimes(config, output);
        }
        Some(("add", add_matches)) => {
            handle_add(add_matches, config, output);
        }
        Some(("remove", remove_matches)) => {
            handle_remove(remove_matches, config, output);
        }
        Some(("activate", activate_matches)) => {
            handle_activate(activate_matches, config, output);
        }
        Some(("inspect", inspect_matches)) => {
            handle_inspect(inspect_matches, config, output);
        }
        _ => {
            println!("Use 'runtime list' to see available runtimes.");
            println!("Run 'avocadoctl runtime --help' for more information.");
        }
    }
}

fn handle_add(matches: &ArgMatches, config: &Config, output: &OutputManager) {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);

    if let Some(url) = matches.get_one::<String>("url") {
        println!();
        println!("  Adding runtime from {url}");
        println!();

        let auth_token = std::env::var("AVOCADO_TUF_AUTH_TOKEN").ok();
        match update::perform_update(
            url,
            base_path,
            auth_token.as_deref(),
            None,
            output.is_verbose(),
        ) {
            Ok(reboot_required) => {
                if reboot_required {
                    println!();
                    output.step(
                        "Runtime Add",
                        "OS update applied. Rebooting to activate new OS...",
                    );
                    let _ = std::process::Command::new("reboot").status();
                } else {
                    crate::commands::ext::refresh_extensions(config, output);
                    println!();
                    output.success("Runtime Add", "Runtime added successfully.");
                }
            }
            Err(e) => {
                println!();
                output.error("Runtime Add", &format!("{e}"));
                std::process::exit(1);
            }
        }
    } else if let Some(manifest_path) = matches.get_one::<String>("manifest") {
        println!();
        println!("  Adding runtime from manifest: {manifest_path}");
        println!();

        let manifest_content = match std::fs::read_to_string(manifest_path) {
            Ok(c) => c,
            Err(e) => {
                output.error("Runtime Add", &format!("Failed to read manifest: {e}"));
                std::process::exit(1);
            }
        };

        let manifest: RuntimeManifest = match serde_json::from_str(&manifest_content) {
            Ok(m) => m,
            Err(e) => {
                output.error("Runtime Add", &format!("Invalid manifest.json: {e}"));
                std::process::exit(1);
            }
        };

        if let Err(e) = staging::validate_manifest_images(&manifest, base_path) {
            output.error("Runtime Add", &format!("{e}"));
            std::process::exit(1);
        }

        if let Err(e) =
            staging::stage_manifest(&manifest, &manifest_content, base_path, output.is_verbose())
        {
            output.error("Runtime Add", &format!("{e}"));
            std::process::exit(1);
        }

        if let Err(e) = staging::activate_runtime(&manifest.id, base_path) {
            output.error("Runtime Add", &format!("{e}"));
            std::process::exit(1);
        }

        let short_id = &manifest.id[..8.min(manifest.id.len())];
        println!(
            "  Activated runtime: {} {} ({short_id})",
            manifest.runtime.name, manifest.runtime.version,
        );

        crate::commands::ext::refresh_extensions(config, output);
        println!();
        output.success("Runtime Add", "Runtime added successfully.");
    }
}

fn handle_remove(matches: &ArgMatches, config: &Config, output: &OutputManager) {
    let id_prefix = matches.get_one::<String>("id").expect("id is required");
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);

    let runtimes = RuntimeManifest::list_all(base_path);
    let (matched, _is_active) = match resolve_runtime_id(id_prefix, &runtimes, output) {
        Some(m) => m,
        None => return,
    };

    if let Err(e) = staging::remove_runtime(&matched.id, base_path) {
        output.error("Runtime Remove", &format!("{e}"));
        std::process::exit(1);
    }

    let short_id = &matched.id[..8.min(matched.id.len())];
    println!();
    output.success(
        "Runtime Remove",
        &format!(
            "Removed runtime: {} {} ({short_id})",
            matched.runtime.name, matched.runtime.version,
        ),
    );
}

fn handle_activate(matches: &ArgMatches, config: &Config, output: &OutputManager) {
    let id_prefix = matches.get_one::<String>("id").expect("id is required");
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);

    let runtimes = RuntimeManifest::list_all(base_path);
    let (matched, is_active) = match resolve_runtime_id(id_prefix, &runtimes, output) {
        Some(m) => m,
        None => return,
    };

    let short_id = &matched.id[..8.min(matched.id.len())];

    if is_active {
        output.info(
            "Runtime Activate",
            &format!(
                "Runtime {} {} ({short_id}) is already active.",
                matched.runtime.name, matched.runtime.version,
            ),
        );
        return;
    }

    if let Err(e) = staging::activate_runtime(&matched.id, base_path) {
        output.error("Runtime Activate", &format!("{e}"));
        std::process::exit(1);
    }

    println!(
        "  Activated runtime: {} {} ({short_id})",
        matched.runtime.name, matched.runtime.version,
    );

    crate::commands::ext::refresh_extensions(config, output);
    println!();
    output.success(
        "Runtime Activate",
        &format!(
            "Switched to runtime: {} {} ({short_id})",
            matched.runtime.name, matched.runtime.version,
        ),
    );
}

fn handle_inspect(matches: &ArgMatches, config: &Config, output: &OutputManager) {
    let id_prefix = matches.get_one::<String>("id").expect("id is required");
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);

    let runtimes = RuntimeManifest::list_all(base_path);
    let (matched, is_active) = match resolve_runtime_id(id_prefix, &runtimes, output) {
        Some(m) => m,
        None => return,
    };

    if output.is_json() {
        match serde_json::to_string_pretty(matched) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                output.error("Runtime Inspect", &format!("Failed to serialize: {e}"));
                std::process::exit(1);
            }
        }
        return;
    }

    let short_id = if matched.id.len() >= 8 {
        &matched.id[..8]
    } else {
        &matched.id
    };

    let active_marker = if is_active { " (active)" } else { "" };

    println!();
    println!(
        "  Runtime: {} {} ({short_id}){active_marker}",
        matched.runtime.name, matched.runtime.version
    );
    println!("  Build ID: {}", matched.id);
    println!("  Built:    {}", matched.built_at);
    println!("  Manifest: v{}", matched.manifest_version);
    println!();

    if matched.extensions.is_empty() {
        println!("  No extensions.");
    } else {
        let name_width = matched
            .extensions
            .iter()
            .map(|e| e.name.len())
            .max()
            .unwrap_or(4)
            .max(4); // at least as wide as "NAME"

        println!(
            "  {:<nw$} {:<12} {:<10}",
            "NAME",
            "VERSION",
            "IMAGE ID",
            nw = name_width
        );

        for ext in &matched.extensions {
            let short_image_id = match &ext.image_id {
                Some(id) if id.len() >= 8 => &id[..8],
                Some(id) => id.as_str(),
                None => "-",
            };
            println!(
                "  {:<nw$} {:<12} {:<10}",
                ext.name,
                ext.version,
                short_image_id,
                nw = name_width
            );
        }
    }

    println!();

    if output.is_verbose() {
        println!("  Full image IDs:");
        for ext in &matched.extensions {
            let id_display = ext.image_id.as_deref().unwrap_or("-");
            println!("    {} {}: {}", ext.name, ext.version, id_display);
        }
        println!();
    }
}

/// Resolve a runtime ID prefix to a unique runtime from the list.
/// Returns the matched runtime manifest and its active status, or None on error.
fn resolve_runtime_id<'a>(
    id_prefix: &str,
    runtimes: &'a [(RuntimeManifest, bool)],
    output: &OutputManager,
) -> Option<(&'a RuntimeManifest, bool)> {
    let matches: Vec<&(RuntimeManifest, bool)> = runtimes
        .iter()
        .filter(|(m, _)| m.id.starts_with(id_prefix))
        .collect();

    match matches.len() {
        0 => {
            output.error(
                "Runtime",
                &format!("No runtime found with ID starting with '{id_prefix}'."),
            );
            std::process::exit(1);
        }
        1 => Some((&matches[0].0, matches[0].1)),
        _ => {
            let ids: Vec<String> = matches
                .iter()
                .map(|(m, active)| {
                    let marker = if *active { " (active)" } else { "" };
                    let sid = &m.id[..8.min(m.id.len())];
                    format!(
                        "  {} {} ({sid}){}",
                        m.runtime.name, m.runtime.version, marker
                    )
                })
                .collect();
            output.error(
                "Runtime",
                &format!(
                    "Ambiguous runtime ID '{id_prefix}', matches:\n{}",
                    ids.join("\n")
                ),
            );
            std::process::exit(1);
        }
    }
}

fn list_runtimes(config: &Config, output: &OutputManager) {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);

    let runtimes = RuntimeManifest::list_all(base_path);

    if output.is_json() {
        let json_runtimes: Vec<serde_json::Value> = runtimes
            .iter()
            .map(|(m, is_active)| {
                serde_json::json!({
                    "id": m.id,
                    "name": m.runtime.name,
                    "version": m.runtime.version,
                    "built_at": m.built_at,
                    "active": is_active,
                    "manifest_version": m.manifest_version,
                    "extensions": m.extensions.len(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_runtimes).unwrap());
        return;
    }

    if runtimes.is_empty() {
        output.info(
            "Runtime List",
            "No runtimes found. Build and provision a runtime first.",
        );
        return;
    }

    println!();
    println!("  {:<32} {:<12} BUILT AT", "RUNTIME", "ACTIVE");

    for (manifest, is_active) in &runtimes {
        let short_id = &manifest.id[..8.min(manifest.id.len())];
        let runtime_label = format!(
            "{} {} ({short_id})",
            manifest.runtime.name, manifest.runtime.version
        );

        let built_at_display = manifest.built_at.replace('T', " ").replace('Z', "");
        let status = if *is_active { "* active" } else { "" };

        println!(
            "  {:<32} {:<12} {}",
            runtime_label, status, built_at_display
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
    use crate::manifest::{ManifestExtension, RuntimeInfo};

    fn make_runtime(id: &str, name: &str, version: &str, built_at: &str) -> RuntimeManifest {
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
                image_id: Some("img-id".to_string()),
            }],
            os_bundle: None,
        }
    }

    #[test]
    fn test_resolve_runtime_id_exact_match() {
        let runtimes = vec![
            (
                make_runtime("abcd1234-5678", "dev", "0.1.0", "2026-02-19T00:00:00Z"),
                true,
            ),
            (
                make_runtime("efgh5678-1234", "prod", "1.0.0", "2026-02-18T00:00:00Z"),
                false,
            ),
        ];
        let output = OutputManager::new(false, false);
        let result = resolve_runtime_id("abcd1234-5678", &runtimes, &output);
        assert!(result.is_some());
        let (m, active) = result.unwrap();
        assert_eq!(m.id, "abcd1234-5678");
        assert!(active);
    }

    #[test]
    fn test_resolve_runtime_id_prefix_match() {
        let runtimes = vec![
            (
                make_runtime("abcd1234-5678", "dev", "0.1.0", "2026-02-19T00:00:00Z"),
                false,
            ),
            (
                make_runtime("efgh5678-1234", "prod", "1.0.0", "2026-02-18T00:00:00Z"),
                true,
            ),
        ];
        let output = OutputManager::new(false, false);
        let result = resolve_runtime_id("abcd", &runtimes, &output);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "abcd1234-5678");
    }
}
