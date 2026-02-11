use crate::config::Config;
use crate::output::OutputManager;
use clap::{Arg, ArgMatches, Command};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

/// Runtime manifest loaded from manifest.json
#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    runtime: String,
    version: String,
    rootfs: String,
    rootfs_id: Option<String>,
}

/// Default sysroot mount point used in the initramfs
const SYSROOT_PATH: &str = "/sysroot";

/// Create the `boot` subcommand
pub fn create_command() -> Command {
    Command::new("boot")
        .about("Mount the rootfs from the var partition at /sysroot during initramfs boot")
        .arg(
            Arg::new("runtime")
                .long("runtime")
                .value_name("NAME")
                .help("Runtime name to boot (auto-detected if only one exists)"),
        )
        .arg(
            Arg::new("sysroot")
                .long("sysroot")
                .value_name("PATH")
                .help("Mount point for the root filesystem (default: /sysroot)"),
        )
        .arg(
            Arg::new("dry_run")
                .long("dry-run")
                .help("Print what would be done without actually mounting")
                .action(clap::ArgAction::SetTrue),
        )
}

/// Handle the `boot` subcommand
pub fn handle_command(matches: &ArgMatches, config: &Config, output: &OutputManager) {
    let sysroot = matches
        .get_one::<String>("sysroot")
        .map(|s| s.as_str())
        .unwrap_or(SYSROOT_PATH);
    let runtime_name = matches.get_one::<String>("runtime").map(|s| s.as_str());
    let dry_run = matches.get_flag("dry_run");

    // Verify we are running in the initramfs
    if !is_running_in_initrd() {
        output.error(
            "Boot",
            "Not running in initramfs (/etc/initrd-release not found). \
             The boot command should only be run from the initramfs.",
        );
        std::process::exit(1);
    }

    output.info("Boot", "Starting rootfs mount from var partition");

    let runtimes_dir = config.get_runtimes_dir();
    let runtimes_path = Path::new(&runtimes_dir);

    if !runtimes_path.exists() {
        output.error(
            "Boot",
            &format!("Runtimes directory not found: {runtimes_dir}"),
        );
        std::process::exit(1);
    }

    // Discover or select the runtime
    let manifest = match resolve_runtime(runtimes_path, runtime_name, output) {
        Some(m) => m,
        None => std::process::exit(1),
    };

    output.info(
        "Boot",
        &format!(
            "Selected runtime '{}' version '{}'",
            manifest.runtime, manifest.version
        ),
    );

    if let Some(ref rootfs_id) = manifest.rootfs_id {
        output.info("Boot", &format!("Rootfs ID: {rootfs_id}"));
    }

    // Resolve the squashfs path
    let runtime_dir = runtimes_path.join(&manifest.runtime);
    let rootfs_path = runtime_dir.join(&manifest.rootfs);

    if !rootfs_path.exists() {
        output.error(
            "Boot",
            &format!(
                "Rootfs image not found: {}",
                rootfs_path.display()
            ),
        );
        std::process::exit(1);
    }

    output.step(
        "Mount",
        &format!(
            "{} -> {sysroot}",
            rootfs_path.display()
        ),
    );

    if dry_run {
        output.success(
            "Boot",
            &format!(
                "[dry-run] Would mount {} at {sysroot}",
                rootfs_path.display()
            ),
        );
        return;
    }

    // Mount the rootfs squashfs at /sysroot
    if let Err(e) = mount_rootfs(&rootfs_path, sysroot, output) {
        output.error("Boot", &format!("Failed to mount rootfs: {e}"));
        std::process::exit(1);
    }

    output.success(
        "Boot",
        &format!(
            "Mounted runtime '{}' at {sysroot}",
            manifest.runtime
        ),
    );
}

/// Detect if we are running in the initrd by checking for /etc/initrd-release
fn is_running_in_initrd() -> bool {
    Path::new("/etc/initrd-release").exists()
}

/// Discover available runtimes and select the appropriate one
fn resolve_runtime(
    runtimes_dir: &Path,
    requested_name: Option<&str>,
    output: &OutputManager,
) -> Option<RuntimeManifest> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();

    let entries = match fs::read_dir(runtimes_dir) {
        Ok(entries) => entries,
        Err(e) => {
            output.error(
                "Boot",
                &format!(
                    "Failed to read runtimes directory '{}': {e}",
                    runtimes_dir.display()
                ),
            );
            return None;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let manifest_path = path.join("manifest.json");
            if manifest_path.exists() {
                let name = entry.file_name().to_string_lossy().to_string();
                found.push((name, manifest_path));
            }
        }
    }

    if found.is_empty() {
        output.error(
            "Boot",
            &format!(
                "No runtimes found in {}",
                runtimes_dir.display()
            ),
        );
        return None;
    }

    let manifest_path = if let Some(name) = requested_name {
        // User specified a runtime name
        match found.iter().find(|(n, _)| n == name) {
            Some((_, path)) => path.clone(),
            None => {
                let available: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
                output.error(
                    "Boot",
                    &format!(
                        "Runtime '{name}' not found. Available: {}",
                        available.join(", ")
                    ),
                );
                return None;
            }
        }
    } else if found.len() == 1 {
        // Auto-select the only available runtime
        let (ref name, ref path) = found[0];
        output.info("Boot", &format!("Auto-selected runtime '{name}'"));
        path.clone()
    } else {
        // Multiple runtimes, require explicit selection
        let available: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
        output.error(
            "Boot",
            &format!(
                "Multiple runtimes found. Use --runtime to select one: {}",
                available.join(", ")
            ),
        );
        return None;
    };

    // Parse the manifest
    let content = match fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(e) => {
            output.error(
                "Boot",
                &format!(
                    "Failed to read manifest '{}': {e}",
                    manifest_path.display()
                ),
            );
            return None;
        }
    };

    match serde_json::from_str::<RuntimeManifest>(&content) {
        Ok(manifest) => Some(manifest),
        Err(e) => {
            output.error(
                "Boot",
                &format!(
                    "Failed to parse manifest '{}': {e}",
                    manifest_path.display()
                ),
            );
            None
        }
    }
}

/// Mount the rootfs squashfs at the specified sysroot path
fn mount_rootfs(squashfs_path: &Path, sysroot: &str, output: &OutputManager) -> Result<(), String> {
    let sysroot_path = Path::new(sysroot);

    // Ensure sysroot directory exists
    if !sysroot_path.exists() {
        fs::create_dir_all(sysroot_path).map_err(|e| {
            format!("Failed to create sysroot directory '{}': {e}", sysroot)
        })?;
    }

    output.step("Mount", &format!("Mounting squashfs at {sysroot}"));

    let result = ProcessCommand::new("mount")
        .args(["-t", "squashfs", "-o", "ro,loop"])
        .arg(squashfs_path)
        .arg(sysroot)
        .output()
        .map_err(|e| format!("Failed to execute mount command: {e}"))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!(
            "mount returned exit code {}: {}",
            result.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    Ok(())
}
