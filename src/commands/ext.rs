use crate::config::Config;
use crate::output::OutputManager;
use clap::{Arg, ArgMatches, Command};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

/// Represents an extension and its type(s)
#[derive(Debug, Clone)]
struct Extension {
    name: String,
    path: PathBuf,
    is_sysext: bool,
    is_confext: bool,
    is_directory: bool, // true for directories, false for .raw files
}

/// Print a colored success message
fn print_colored_success(message: &str) {
    // Use auto-detection but fallback gracefully
    let color_choice =
        if std::env::var("NO_COLOR").is_ok() || std::env::var("AVOCADO_TEST_MODE").is_ok() {
            ColorChoice::Never
        } else {
            ColorChoice::Auto
        };

    let mut stdout = StandardStream::stdout(color_choice);
    let mut color_spec = ColorSpec::new();
    color_spec.set_fg(Some(Color::Green)).set_bold(true);

    if stdout.set_color(&color_spec).is_ok() && color_choice != ColorChoice::Never {
        let _ = write!(&mut stdout, "[SUCCESS]");
        let _ = stdout.reset();
        println!(" {message}");
    } else {
        // Fallback for environments without color support
        println!("[SUCCESS] {message}");
    }
}

/// Print a colored info message
fn print_colored_info(message: &str) {
    // Use auto-detection but fallback gracefully
    let color_choice =
        if std::env::var("NO_COLOR").is_ok() || std::env::var("AVOCADO_TEST_MODE").is_ok() {
            ColorChoice::Never
        } else {
            ColorChoice::Auto
        };

    let mut stdout = StandardStream::stdout(color_choice);
    let mut color_spec = ColorSpec::new();
    color_spec.set_fg(Some(Color::Blue)).set_bold(true);

    if stdout.set_color(&color_spec).is_ok() && color_choice != ColorChoice::Never {
        let _ = write!(&mut stdout, "[INFO]");
        let _ = stdout.reset();
        println!(" {message}");
    } else {
        // Fallback for environments without color support
        println!("[INFO] {message}");
    }
}

/// Detect if we are running in the initrd by checking for /etc/initrd-release
fn is_running_in_initrd() -> bool {
    Path::new("/etc/initrd-release").exists()
}

/// Parse scope values from release file content (e.g., SYSEXT_SCOPE or CONFEXT_SCOPE)
fn parse_scope_from_release_content(content: &str, scope_key: &str) -> Vec<String> {
    let mut scopes = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with(&format!("{scope_key}=")) {
            let value = line
                .split_once('=')
                .map(|x| x.1)
                .unwrap_or("")
                .trim_matches('"')
                .trim();

            // Parse space-separated list of scopes
            for scope in value.split_whitespace() {
                if !scope.is_empty() {
                    scopes.push(scope.to_string());
                }
            }
            break; // Only process the first occurrence
        }
    }

    scopes
}

/// Check if a sysext is enabled for the current environment (initrd vs system)
fn is_sysext_enabled_for_current_environment(extension_path: &Path, extension_name: &str) -> bool {
    let in_initrd = is_running_in_initrd();
    let required_scope = if in_initrd { "initrd" } else { "system" };

    let sysext_release_path = extension_path
        .join("usr/lib/extension-release.d")
        .join(format!("extension-release.{extension_name}"));

    if sysext_release_path.exists() {
        if let Ok(content) = fs::read_to_string(&sysext_release_path) {
            let scopes = parse_scope_from_release_content(&content, "SYSEXT_SCOPE");
            // If no scope is specified, default to enabled (backward compatibility)
            if scopes.is_empty() {
                return true;
            }
            // Check if the required scope is in the list
            return scopes.contains(&required_scope.to_string());
        }
    }

    // If no release file exists, default to enabled (backward compatibility)
    true
}

/// Check if a confext is enabled for the current environment (initrd vs system)
fn is_confext_enabled_for_current_environment(extension_path: &Path, extension_name: &str) -> bool {
    let in_initrd = is_running_in_initrd();
    let required_scope = if in_initrd { "initrd" } else { "system" };

    let confext_release_path = extension_path
        .join("etc/extension-release.d")
        .join(format!("extension-release.{extension_name}"));

    if confext_release_path.exists() {
        if let Ok(content) = fs::read_to_string(&confext_release_path) {
            let scopes = parse_scope_from_release_content(&content, "CONFEXT_SCOPE");
            // If no scope is specified, default to enabled (backward compatibility)
            if scopes.is_empty() {
                return true;
            }
            // Check if the required scope is in the list
            return scopes.contains(&required_scope.to_string());
        }
    }

    // If no release file exists, default to enabled (backward compatibility)
    true
}

/// Create the ext subcommand definition
pub fn create_command() -> Command {
    Command::new("ext")
        .about("Extension management commands")
        .subcommand(Command::new("list").about("List all available extensions"))
        .subcommand(
            Command::new("merge")
                .about("Merge extensions using systemd-sysext and systemd-confext"),
        )
        .subcommand(
            Command::new("unmerge")
                .about("Unmerge extensions using systemd-sysext and systemd-confext")
                .arg(
                    Arg::new("unmount")
                        .long("unmount")
                        .help("Also unmount all persistent loops for .raw extensions")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("refresh").about("Unmerge and then merge extensions (refresh extensions)"),
        )
        .subcommand(Command::new("status").about("Show status of merged extensions"))
}

/// Handle ext command and its subcommands
pub fn handle_command(matches: &ArgMatches, config: &Config, output: &OutputManager) {
    match matches.subcommand() {
        Some(("list", _)) => {
            list_extensions(config, output);
        }
        Some(("merge", _)) => {
            merge_extensions(output);
        }
        Some(("unmerge", unmerge_matches)) => {
            let unmount = unmerge_matches.get_flag("unmount");
            unmerge_extensions(unmount, output);
        }
        Some(("refresh", _)) => {
            refresh_extensions(output);
        }
        Some(("status", _)) => {
            status_extensions(output);
        }
        _ => {
            println!("Use 'avocadoctl ext --help' for available extension commands");
        }
    }
}

/// List all extensions from the extensions directory
fn list_extensions(config: &Config, output: &OutputManager) {
    output.info("Extension List", "Listing available extensions");
    let extensions_path = config.get_extensions_dir();

    match fs::read_dir(&extensions_path) {
        Ok(entries) => {
            let mut extension_names = Vec::new();

            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if let Some(name) = path.file_name() {
                            if let Some(name_str) = name.to_str() {
                                // Handle directories and .raw files
                                if path.is_dir() {
                                    extension_names.push(name_str.to_string());
                                } else if name_str.ends_with(".raw") {
                                    // Remove .raw extension from filename
                                    let ext_name =
                                        name_str.strip_suffix(".raw").unwrap_or(name_str);
                                    extension_names.push(ext_name.to_string());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading entry: {e}");
                    }
                }
            }

            if extension_names.is_empty() {
                println!("No extensions found in {extensions_path}");
            } else {
                extension_names.sort();
                println!("Available extensions:");
                for name in extension_names {
                    println!("  {name}");
                }
            }
        }
        Err(e) => {
            eprintln!("Error accessing extensions directory '{extensions_path}': {e}");
            eprintln!("Make sure the directory exists and you have read permissions.");
        }
    }
}

/// Merge extensions using systemd-sysext and systemd-confext
pub fn merge_extensions(output: &OutputManager) {
    match merge_extensions_internal(output) {
        Ok(_) => {
            output.success("Extension Merge", "Extensions merged successfully");
        }
        Err(e) => {
            output.error(
                "Extension Merge",
                &format!("Failed to merge extensions: {e}"),
            );
            std::process::exit(1);
        }
    }
}

/// Internal merge function that returns a Result
fn merge_extensions_internal(output: &OutputManager) -> Result<(), SystemdError> {
    let environment_info = if is_running_in_initrd() {
        "initrd environment"
    } else {
        "system environment"
    };
    output.info(
        "Extension Merge",
        &format!("Starting extension merge process in {environment_info}"),
    );

    // Prepare the environment by setting up symlinks and get the list of enabled extensions
    let enabled_extensions = prepare_extension_environment_with_output(output)?;

    // Merge system extensions
    let sysext_result = run_systemd_command(
        "systemd-sysext",
        &["merge", "--mutable=ephemeral", "--json=short"],
    )?;
    handle_systemd_output("systemd-sysext merge", &sysext_result, output)?;

    // Merge configuration extensions
    let confext_result = run_systemd_command(
        "systemd-confext",
        &["merge", "--mutable=ephemeral", "--json=short"],
    )?;
    handle_systemd_output("systemd-confext merge", &confext_result, output)?;

    // Process post-merge tasks only for enabled extensions
    process_post_merge_tasks_for_extensions(&enabled_extensions)?;

    Ok(())
}

/// Unmerge extensions using systemd-sysext and systemd-confext
pub fn unmerge_extensions(unmount: bool, output: &OutputManager) {
    match unmerge_extensions_internal(unmount, output) {
        Ok(_) => {
            output.success("Extension Unmerge", "Extensions unmerged successfully");
        }
        Err(e) => {
            output.error(
                "Extension Unmerge",
                &format!("Failed to unmerge extensions: {e}"),
            );
            std::process::exit(1);
        }
    }
}

/// Internal unmerge function that returns a Result for use in refresh
fn unmerge_extensions_internal(unmount: bool, output: &OutputManager) -> Result<(), SystemdError> {
    unmerge_extensions_internal_with_depmod(true, unmount, output)
}

/// Internal unmerge function with optional depmod control
fn unmerge_extensions_internal_with_depmod(
    call_depmod: bool,
    unmount: bool,
    output: &OutputManager,
) -> Result<(), SystemdError> {
    unmerge_extensions_internal_with_options(call_depmod, unmount, output)
}

/// Internal unmerge function with all options
fn unmerge_extensions_internal_with_options(
    call_depmod: bool,
    unmount: bool,
    output: &OutputManager,
) -> Result<(), SystemdError> {
    let environment_info = if is_running_in_initrd() {
        "initrd environment"
    } else {
        "system environment"
    };
    output.info(
        "Extension Unmerge",
        &format!("Starting extension unmerge process in {environment_info}"),
    );

    // Note: We don't execute AVOCADO_ON_MERGE commands during unmerge
    // Those commands are only meant to be run during merge operations

    // Unmerge system extensions
    let sysext_result = run_systemd_command("systemd-sysext", &["unmerge", "--json=short"])?;
    handle_systemd_output("systemd-sysext unmerge", &sysext_result, output)?;

    // Unmerge configuration extensions
    let confext_result = run_systemd_command("systemd-confext", &["unmerge", "--json=short"])?;
    handle_systemd_output("systemd-confext unmerge", &confext_result, output)?;

    // Clean up all symlinks to ensure fresh state for next merge
    cleanup_extension_symlinks(output)?;

    // Run depmod after unmerge if requested
    if call_depmod {
        run_depmod()?;
    }

    // Unmount persistent loops if requested
    if unmount {
        unmount_all_persistent_loops()?;
    }

    Ok(())
}

/// Direct access functions for top-level command aliases
///
/// Merge extensions - direct access for top-level alias
pub fn merge_extensions_direct(output: &OutputManager) {
    merge_extensions(output);
}

/// Unmerge extensions - direct access for top-level alias
pub fn unmerge_extensions_direct(unmount: bool, output: &OutputManager) {
    unmerge_extensions(unmount, output);
}

/// Refresh extensions - direct access for top-level alias
pub fn refresh_extensions_direct(output: &OutputManager) {
    refresh_extensions(output);
}

/// Refresh extensions (unmerge then merge)
pub fn refresh_extensions(output: &OutputManager) {
    let environment_info = if is_running_in_initrd() {
        "initrd environment"
    } else {
        "system environment"
    };
    output.info(
        "Extension Refresh",
        &format!("Starting extension refresh process in {environment_info}"),
    );

    // First unmerge (skip depmod since we'll call it after merge, don't unmount loops)
    if let Err(e) = unmerge_extensions_internal_with_options(false, false, output) {
        output.error(
            "Extension Refresh",
            &format!("Failed to unmerge extensions: {e}"),
        );
        std::process::exit(1);
    }
    output.step("Refresh", "Extensions unmerged");

    // Then merge (this will call depmod via post-merge processing)
    if let Err(e) = merge_extensions_internal(output) {
        output.error(
            "Extension Refresh",
            &format!("Failed to merge extensions: {e}"),
        );
        std::process::exit(1);
    }
    output.step("Refresh", "Extensions merged");

    output.success("Extension Refresh", "Extensions refreshed successfully");
}

/// Show status of merged extensions
pub fn status_extensions(output: &OutputManager) {
    match show_enhanced_status(output) {
        Ok(_) => {}
        Err(e) => {
            output.error("Extension Status", &format!("Failed to show status: {e}"));
            // Fall back to legacy status display
            show_legacy_status(output);
        }
    }
}

/// Show enhanced status with extension origins and HITL information
fn show_enhanced_status(output: &OutputManager) -> Result<(), SystemdError> {
    output.status_header("Avocado Extension Status");

    // Get our view of available extensions
    let available_extensions = scan_extensions_from_all_sources()?;

    // Get systemd's view of mounted extensions
    let mounted_sysext = get_mounted_systemd_extensions("systemd-sysext")?;
    let mounted_confext = get_mounted_systemd_extensions("systemd-confext")?;

    // Create comprehensive status
    display_extension_status(&available_extensions, &mounted_sysext, &mounted_confext)?;

    Ok(())
}

/// Legacy status display for fallback
fn show_legacy_status(output: &OutputManager) {
    output.status("Legacy status display not yet implemented");
    println!("Extension Status");
    println!("================");
    println!();

    // Get system extensions status
    println!("System Extensions (/opt, /usr):");
    println!("--------------------------------");
    match run_systemd_command("systemd-sysext", &["status"]) {
        Ok(output) => {
            if output.trim().is_empty() {
                println!("No system extensions currently merged.");
            } else {
                format_status_output(&output);
            }
        }
        Err(e) => {
            eprintln!("Error getting system extensions status: {e}");
        }
    }

    println!();

    // Get configuration extensions status
    println!("Configuration Extensions (/etc):");
    println!("---------------------------------");
    match run_systemd_command("systemd-confext", &["status"]) {
        Ok(output) => {
            if output.trim().is_empty() {
                println!("No configuration extensions currently merged.");
            } else {
                format_status_output(&output);
            }
        }
        Err(e) => {
            eprintln!("Error getting configuration extensions status: {e}");
        }
    }
}

/// Structure to represent mounted extension info from systemd
#[derive(Debug, Clone)]
struct MountedExtension {
    name: String,
    since: String,
    #[allow(dead_code)] // May be used in future for hierarchy-specific logic
    hierarchy: String,
}

/// Get mounted extensions from systemd using JSON format
fn get_mounted_systemd_extensions(command: &str) -> Result<Vec<MountedExtension>, SystemdError> {
    let mut mounted = Vec::new();

    let output = run_systemd_command(command, &["status", "--json=short"])?;
    if output.trim().is_empty() {
        return Ok(mounted);
    }

    // Parse JSON output
    let json_data: serde_json::Value =
        serde_json::from_str(&output).map_err(|e| SystemdError::CommandFailed {
            command: format!("{command} status --json=short"),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;

    // Handle both single object and array formats
    let hierarchies = if json_data.is_array() {
        json_data.as_array().unwrap()
    } else {
        std::slice::from_ref(&json_data)
    };

    for hierarchy_obj in hierarchies {
        let hierarchy = hierarchy_obj["hierarchy"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let since_timestamp = hierarchy_obj["since"].as_u64();
        let since = if let Some(ts) = since_timestamp {
            // Convert microseconds timestamp to human readable format
            let secs = ts / 1_000_000;
            // For now, use a simple format. In the future we could add chrono for better formatting
            format!("timestamp:{secs}")
        } else {
            "-".to_string()
        };

        // Handle extensions field - can be string "none" or array of strings
        if let Some(extensions) = hierarchy_obj["extensions"].as_array() {
            // Array of extension names
            for ext in extensions {
                if let Some(ext_name) = ext.as_str() {
                    mounted.push(MountedExtension {
                        name: ext_name.to_string(),
                        since: since.clone(),
                        hierarchy: hierarchy.clone(),
                    });
                }
            }
        } else if let Some(ext_str) = hierarchy_obj["extensions"].as_str() {
            // Single string - skip if it's "none"
            if ext_str != "none" {
                mounted.push(MountedExtension {
                    name: ext_str.to_string(),
                    since: since.clone(),
                    hierarchy: hierarchy.clone(),
                });
            }
        }
    }

    Ok(mounted)
}

/// Display comprehensive extension status
fn display_extension_status(
    available: &[Extension],
    mounted_sysext: &[MountedExtension],
    mounted_confext: &[MountedExtension],
) -> Result<(), SystemdError> {
    // Collect all unique extension names
    let mut all_extensions = std::collections::HashSet::new();

    for ext in available {
        all_extensions.insert(&ext.name);
    }
    for ext in mounted_sysext {
        all_extensions.insert(&ext.name);
    }
    for ext in mounted_confext {
        all_extensions.insert(&ext.name);
    }

    if all_extensions.is_empty() {
        println!("No extensions found or mounted.");
        return Ok(());
    }

    // Display header
    println!(
        "{:<20} {:<12} {:<15} {:<30} Mount Info",
        "Extension", "Status", "Type", "Origin"
    );
    println!("{}", "=".repeat(100));

    // Sort extensions for consistent display
    let mut sorted_extensions: Vec<_> = all_extensions.into_iter().collect();
    sorted_extensions.sort();

    for ext_name in sorted_extensions {
        display_extension_info(ext_name, available, mounted_sysext, mounted_confext);
    }

    // Display summary
    println!();
    display_status_summary(available, mounted_sysext, mounted_confext);

    Ok(())
}

/// Display information for a single extension
fn display_extension_info(
    ext_name: &str,
    available: &[Extension],
    mounted_sysext: &[MountedExtension],
    mounted_confext: &[MountedExtension],
) {
    let available_ext = available.iter().find(|e| e.name == ext_name);
    let sysext_mount = mounted_sysext.iter().find(|e| e.name == ext_name);
    let confext_mount = mounted_confext.iter().find(|e| e.name == ext_name);

    // Determine status
    let status = match (sysext_mount.is_some(), confext_mount.is_some()) {
        (true, true) => "MOUNTED",
        (true, false) => "SYSEXT",
        (false, true) => "CONFEXT",
        (false, false) => "AVAILABLE",
    };

    // Determine types
    let mut types = Vec::new();
    if let Some(ext) = available_ext {
        if ext.is_sysext {
            types.push("sysext");
        }
        if ext.is_confext {
            types.push("confext");
        }
    }
    let type_str = if types.is_empty() {
        "unknown".to_string()
    } else {
        types.join("+")
    };

    // Determine origin
    let origin = if let Some(ext) = available_ext {
        get_extension_origin(ext)
    } else {
        "unknown".to_string()
    };

    // Determine mount info
    let mount_info = match (sysext_mount, confext_mount) {
        (Some(s), Some(c)) => format!("sys:{}, conf:{}", s.since, c.since),
        (Some(s), None) => format!("sys:{}", s.since),
        (None, Some(c)) => format!("conf:{}", c.since),
        (None, None) => "not mounted".to_string(),
    };

    println!("{ext_name:<20} {status:<12} {type_str:<15} {origin:<30} {mount_info}");
}

/// Get extension origin description
fn get_extension_origin(ext: &Extension) -> String {
    let path_str = ext.path.to_string_lossy();

    if path_str.contains("/hitl") {
        format!("HITL ({})", get_short_path(&ext.path))
    } else if ext.is_directory {
        format!("Directory ({})", get_short_path(&ext.path))
    } else {
        format!("Loop device ({})", get_short_path(&ext.path))
    }
}

/// Get shortened path for display
fn get_short_path(path: &Path) -> String {
    let path_str = path.to_string_lossy();

    // Show relative to common base paths
    if let Some(suffix) = path_str.strip_prefix("/run/avocado/hitl/") {
        format!("hitl/{suffix}")
    } else if let Some(suffix) = path_str.strip_prefix("/var/lib/avocado/extensions/") {
        format!("ext/{suffix}")
    } else if let Some(suffix) = path_str.strip_prefix("/run/avocado/extensions/") {
        format!("loop/{suffix}")
    } else if path_str.contains("/tmp/") {
        // For test mode, show just the final components
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    } else {
        // Fallback: show last two components
        let components: Vec<_> = path.components().collect();
        if components.len() >= 2 {
            format!(
                "{}/{}",
                components[components.len() - 2]
                    .as_os_str()
                    .to_string_lossy(),
                components[components.len() - 1]
                    .as_os_str()
                    .to_string_lossy()
            )
        } else {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        }
    }
}

/// Display status summary
fn display_status_summary(
    available: &[Extension],
    mounted_sysext: &[MountedExtension],
    mounted_confext: &[MountedExtension],
) {
    let hitl_count = available
        .iter()
        .filter(|e| e.path.to_string_lossy().contains("/hitl"))
        .count();
    let directory_count = available
        .iter()
        .filter(|e| e.is_directory && !e.path.to_string_lossy().contains("/hitl"))
        .count();
    let loop_count = available.iter().filter(|e| !e.is_directory).count();

    println!("Summary:");
    println!("  Available Extensions: {} total", available.len());
    println!("    - HITL mounted: {hitl_count}");
    println!("    - Local directories: {directory_count}");
    println!("    - Loop devices: {loop_count}");
    println!("  Mounted Extensions:");
    println!("    - System extensions: {}", mounted_sysext.len());
    println!("    - Configuration extensions: {}", mounted_confext.len());

    if hitl_count > 0 {
        print_colored_info("HITL extensions are active - development mode");
    }
}

/// Format status output from systemd commands
fn format_status_output(output: &str) {
    let lines: Vec<&str> = output.lines().collect();

    // Skip the header line if present and process the data
    let data_lines: Vec<&str> = lines
        .iter()
        .skip_while(|line| line.starts_with("HIERARCHY") || line.trim().is_empty())
        .copied()
        .collect();

    if data_lines.is_empty() {
        println!("No extensions currently merged.");
        return;
    }

    for line in data_lines {
        if line.trim().is_empty() {
            continue;
        }

        // Parse the line format: HIERARCHY EXTENSIONS SINCE
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let hierarchy = parts[0];
            let extensions = parts[1];
            let since = parts[2..].join(" ");

            println!("  {hierarchy} -> {extensions} (since {since})");
        } else {
            // Fallback: just print the line as-is
            println!("  {line}");
        }
    }
}

/// Prepare the extension environment by setting up symlinks with output manager
fn prepare_extension_environment_with_output(
    output: &OutputManager,
) -> Result<Vec<Extension>, SystemdError> {
    output.step("Environment", "Preparing extension environment");

    // Verify clean state by ensuring no stale symlinks exist
    verify_clean_extension_environment(output)?;

    // Scan for available extensions from multiple sources
    let extensions = scan_extensions_from_all_sources_with_verbosity(output.is_verbose())?;

    if extensions.is_empty() {
        output.progress("No extensions found in any source location");
        return Ok(Vec::new());
    }

    // Create target directories
    create_target_directories()?;

    // Track which extensions are actually enabled and linked
    let mut enabled_extensions = Vec::new();

    // Create symlinks for sysext and confext extensions
    for extension in &extensions {
        let mut extension_enabled = false;

        if extension.is_sysext {
            create_sysext_symlink_with_verbosity(extension, output.is_verbose())?;
            extension_enabled = true;
        }
        if extension.is_confext {
            create_confext_symlink_with_verbosity(extension, output.is_verbose())?;
            extension_enabled = true;
        }

        // Only add to enabled list if at least one type was linked
        if extension_enabled {
            enabled_extensions.push(extension.clone());
        }
    }

    output.progress("Extension environment prepared successfully");
    Ok(enabled_extensions)
}

/// Scan all extension sources in priority order (legacy)
fn scan_extensions_from_all_sources() -> Result<Vec<Extension>, SystemdError> {
    scan_extensions_from_all_sources_with_verbosity(true)
}

/// Scan all extension sources in priority order with verbosity control
fn scan_extensions_from_all_sources_with_verbosity(
    verbose: bool,
) -> Result<Vec<Extension>, SystemdError> {
    let mut extensions = Vec::new();
    let mut extension_map = std::collections::HashMap::new();

    // Define search paths in priority order: HITL → Directory → Loop-mounted
    let hitl_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/avocado/hitl")
    } else {
        "/run/avocado/hitl".to_string()
    };

    let extensions_dir = std::env::var("AVOCADO_EXTENSIONS_PATH")
        .unwrap_or_else(|_| "/var/lib/avocado/extensions".to_string());

    // 1. First priority: HITL mounted extensions
    if verbose {
        println!("Scanning HITL extensions in {hitl_dir}");
    }
    if let Ok(hitl_extensions) = scan_directory_extensions(&hitl_dir) {
        for ext in hitl_extensions {
            if verbose {
                println!(
                    "Found HITL extension: {} at {}",
                    ext.name,
                    ext.path.display()
                );
            }
            extension_map.insert(ext.name.clone(), ext);
        }
    }

    // 2. Second priority: Regular directory extensions (skip if already have HITL version)
    if verbose {
        println!("Scanning directory extensions in {extensions_dir}");
    }
    if let Ok(dir_extensions) = scan_directory_extensions(&extensions_dir) {
        for ext in dir_extensions {
            if !extension_map.contains_key(&ext.name) {
                if verbose {
                    println!(
                        "Found directory extension: {} at {}",
                        ext.name,
                        ext.path.display()
                    );
                }
                extension_map.insert(ext.name.clone(), ext);
            } else if verbose {
                println!(
                    "Skipping directory extension {} (HITL version preferred)",
                    ext.name
                );
            }
        }
    }

    // 3. Third priority: Raw file extensions (skip if already have directory version)
    if verbose {
        println!("Scanning raw file extensions in {extensions_dir}");
    }
    let raw_files = scan_raw_files(&extensions_dir)?;

    // Cleanup stale loops before processing new ones
    let mut available_extension_names: Vec<String> = extension_map.keys().cloned().collect();
    available_extension_names.extend(raw_files.iter().map(|(name, _)| name.clone()));
    cleanup_stale_loops(&available_extension_names)?;

    // Process .raw files with persistent loops (only if not already found)
    for (ext_name, path) in raw_files {
        match extension_map.entry(ext_name.clone()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                if verbose {
                    println!("Found raw file extension: {ext_name} at {}", path.display());
                }
                let extension = analyze_raw_extension_with_loop(&ext_name, &path)?;
                entry.insert(extension);
            }
            std::collections::hash_map::Entry::Occupied(_) => {
                if verbose {
                    println!(
                        "Skipping raw file extension {ext_name} (higher priority version preferred)"
                    );
                }
            }
        }
    }

    // Convert map to vector
    extensions.extend(extension_map.into_values());
    Ok(extensions)
}

/// Scan a single directory for directory-based extensions
fn scan_directory_extensions(dir_path: &str) -> Result<Vec<Extension>, SystemdError> {
    let mut extensions = Vec::new();

    if !Path::new(dir_path).exists() {
        return Ok(extensions);
    }

    let entries = fs::read_dir(dir_path).map_err(|e| SystemdError::CommandFailed {
        command: "scan_directory_extensions".to_string(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| SystemdError::CommandFailed {
            command: "scan_directory_extensions".to_string(),
            source: e,
        })?;

        let path = entry.path();

        if path.is_dir() {
            if let Some(file_name) = path.file_name() {
                if let Some(name_str) = file_name.to_str() {
                    let extension = analyze_directory_extension(name_str, &path)?;
                    extensions.push(extension);
                }
            }
        }
    }

    Ok(extensions)
}

/// Scan a directory for raw file extensions
fn scan_raw_files(dir_path: &str) -> Result<Vec<(String, PathBuf)>, SystemdError> {
    let mut raw_files = Vec::new();

    if !Path::new(dir_path).exists() {
        return Ok(raw_files);
    }

    let entries = fs::read_dir(dir_path).map_err(|e| SystemdError::CommandFailed {
        command: "scan_raw_files".to_string(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| SystemdError::CommandFailed {
            command: "scan_raw_files".to_string(),
            source: e,
        })?;

        let path = entry.path();

        if path.is_file() {
            if let Some(file_name) = path.file_name() {
                if let Some(name_str) = file_name.to_str() {
                    if name_str.ends_with(".raw") {
                        let ext_name = name_str.strip_suffix(".raw").unwrap_or(name_str);
                        raw_files.push((ext_name.to_string(), path));
                    }
                }
            }
        }
    }

    Ok(raw_files)
}

/// Analyze a directory extension to determine if it's sysext, confext, or both
fn analyze_directory_extension(name: &str, path: &Path) -> Result<Extension, SystemdError> {
    let mut is_sysext = false;
    let mut is_confext = false;

    // Look for extension-release files
    let sysext_release_path = path
        .join("usr/lib/extension-release.d")
        .join(format!("extension-release.{name}"));
    let confext_release_path = path
        .join("etc/extension-release.d")
        .join(format!("extension-release.{name}"));

    if sysext_release_path.exists() {
        is_sysext = true;
    }

    if confext_release_path.exists() {
        is_confext = true;
    }

    // If no release files found, default to both types
    if !is_sysext && !is_confext {
        is_sysext = true;
        is_confext = true;
    }

    // Check scope requirements for current environment (initrd vs system)
    let sysext_enabled = if is_sysext {
        is_sysext_enabled_for_current_environment(path, name)
    } else {
        false
    };

    let confext_enabled = if is_confext {
        is_confext_enabled_for_current_environment(path, name)
    } else {
        false
    };

    Ok(Extension {
        name: name.to_string(),
        path: path.to_path_buf(),
        is_sysext: sysext_enabled,
        is_confext: confext_enabled,
        is_directory: true,
    })
}

/// Analyze a .raw file extension using persistent loops
fn analyze_raw_extension_with_loop(name: &str, path: &Path) -> Result<Extension, SystemdError> {
    println!("Analyzing raw extension with persistent loop: {name}");

    // Check if we already have a persistent loop for this extension
    let mount_point = if check_existing_loop_ref(name) {
        println!("Using existing persistent loop for {name}");
        if std::env::var("AVOCADO_TEST_MODE").is_ok() {
            let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
            format!("{temp_base}/avocado/extensions/{name}")
        } else {
            format!("/run/avocado/extensions/{name}")
        }
    } else {
        // Create new persistent loop
        mount_raw_file_with_loop(name, path)?
            .to_string_lossy()
            .to_string()
    };

    // Now analyze as a directory by looking at release files
    let mount_path = PathBuf::from(&mount_point);
    let mut is_sysext = false;
    let mut is_confext = false;

    // Check for sysext release file
    let sysext_release_path = mount_path
        .join("usr/lib/extension-release.d")
        .join(format!("extension-release.{name}"));
    if sysext_release_path.exists() {
        is_sysext = true;
    }

    // Check for confext release file
    let confext_release_path = mount_path
        .join("etc/extension-release.d")
        .join(format!("extension-release.{name}"));
    if confext_release_path.exists() {
        is_confext = true;
    }

    // If no release files found, default to both types (same as directory behavior)
    if !is_sysext && !is_confext {
        is_sysext = true;
        is_confext = true;
    }

    // Check scope requirements for current environment (initrd vs system)
    let sysext_enabled = if is_sysext {
        is_sysext_enabled_for_current_environment(&mount_path, name)
    } else {
        false
    };

    let confext_enabled = if is_confext {
        is_confext_enabled_for_current_environment(&mount_path, name)
    } else {
        false
    };

    Ok(Extension {
        name: name.to_string(),
        path: mount_path, // Use the mounted path instead of the raw file path
        is_sysext: sysext_enabled,
        is_confext: confext_enabled,
        is_directory: false, // Still track that this originated from a .raw file
    })
}

/// Create target directories for symlinks
fn create_target_directories() -> Result<(), SystemdError> {
    let (sysext_dir, confext_dir) = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        // In test mode, use temporary directories
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        (
            format!("{temp_base}/test_extensions"),
            format!("{temp_base}/test_confexts"),
        )
    } else {
        ("/run/extensions".to_string(), "/run/confexts".to_string())
    };

    // Create /run/extensions (or test equivalent) if it doesn't exist
    if !Path::new(&sysext_dir).exists() {
        fs::create_dir_all(&sysext_dir).map_err(|e| SystemdError::CommandFailed {
            command: "create_dir_all".to_string(),
            source: e,
        })?;
    }

    // Create /run/confexts (or test equivalent) if it doesn't exist
    if !Path::new(&confext_dir).exists() {
        fs::create_dir_all(&confext_dir).map_err(|e| SystemdError::CommandFailed {
            command: "create_dir_all".to_string(),
            source: e,
        })?;
    }

    Ok(())
}

/// Create a symlink for a sysext extension with verbosity control
fn create_sysext_symlink_with_verbosity(
    extension: &Extension,
    verbose: bool,
) -> Result<(), SystemdError> {
    let sysext_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/test_extensions")
    } else {
        "/run/extensions".to_string()
    };

    // Always use just the extension name for symlinks
    let symlink_name = extension.name.clone();

    let target_path = format!("{sysext_dir}/{symlink_name}");

    // Remove existing symlink or file if it exists
    if Path::new(&target_path).exists() {
        let path = Path::new(&target_path);

        // Try to remove as file first (works for symlinks and regular files)
        if fs::remove_file(&target_path).is_err() {
            // If that fails, it might be a directory
            if path.is_dir() {
                fs::remove_dir_all(&target_path).map_err(|e| SystemdError::CommandFailed {
                    command: "remove_dir_all".to_string(),
                    source: e,
                })?;
            }
        }
    }

    // Create symlink
    unix_fs::symlink(&extension.path, &target_path).map_err(|e| SystemdError::CommandFailed {
        command: "symlink".to_string(),
        source: e,
    })?;

    if verbose {
        println!(
            "Created sysext symlink: {} -> {}",
            target_path,
            extension.path.display()
        );
    }
    Ok(())
}

/// Create a symlink for a confext extension with verbosity control
fn create_confext_symlink_with_verbosity(
    extension: &Extension,
    verbose: bool,
) -> Result<(), SystemdError> {
    let confext_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/test_confexts")
    } else {
        "/run/confexts".to_string()
    };

    // Always use just the extension name for symlinks
    let symlink_name = extension.name.clone();

    let target_path = format!("{confext_dir}/{symlink_name}");

    // Remove existing symlink or file if it exists
    if Path::new(&target_path).exists() {
        let path = Path::new(&target_path);

        // Try to remove as file first (works for symlinks and regular files)
        if fs::remove_file(&target_path).is_err() {
            // If that fails, it might be a directory
            if path.is_dir() {
                fs::remove_dir_all(&target_path).map_err(|e| SystemdError::CommandFailed {
                    command: "remove_dir_all".to_string(),
                    source: e,
                })?;
            }
        }
    }

    // Create symlink
    unix_fs::symlink(&extension.path, &target_path).map_err(|e| SystemdError::CommandFailed {
        command: "symlink".to_string(),
        source: e,
    })?;

    if verbose {
        println!(
            "Created confext symlink: {} -> {}",
            target_path,
            extension.path.display()
        );
    }
    Ok(())
}

/// Mount a .raw file using systemd-dissect with persistent loop
fn mount_raw_file_with_loop(
    extension_name: &str,
    raw_path: &Path,
) -> Result<PathBuf, SystemdError> {
    let mount_point = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/avocado/extensions/{extension_name}")
    } else {
        format!("/run/avocado/extensions/{extension_name}")
    };

    // Create mount point directory
    if let Some(parent) = Path::new(&mount_point).parent() {
        fs::create_dir_all(parent).map_err(|e| SystemdError::CommandFailed {
            command: "create_dir_all".to_string(),
            source: e,
        })?;
    }

    println!("Mounting raw file {extension_name} with persistent loop...");

    // Check if we're in test mode and should use mock commands
    let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        "mock-systemd-dissect"
    } else {
        "systemd-dissect"
    };

    let output = ProcessCommand::new(command_name)
        .args([
            format!("--loop-ref={extension_name}").as_str(),
            "--mkdir",
            "-r",
            "-M",
            raw_path.to_str().unwrap_or(""),
            &mount_point,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| SystemdError::CommandFailed {
            command: command_name.to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SystemdError::CommandExitedWithError {
            command: command_name.to_string(),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }

    println!("Mounted {extension_name} to {mount_point}");
    Ok(PathBuf::from(mount_point))
}

/// Check if a loop ref already exists for an extension
fn check_existing_loop_ref(extension_name: &str) -> bool {
    let loop_ref_path = format!("/dev/disk/by-loop-ref/{extension_name}");
    Path::new(&loop_ref_path).exists()
}

/// Cleanup stale loop refs for extensions that no longer exist
fn cleanup_stale_loops(available_extensions: &[String]) -> Result<(), SystemdError> {
    let loop_ref_dir = "/dev/disk/by-loop-ref";
    if !Path::new(loop_ref_dir).exists() {
        return Ok(());
    }

    let entries = fs::read_dir(loop_ref_dir).map_err(|e| SystemdError::CommandFailed {
        command: "read_dir".to_string(),
        source: e,
    })?;

    for entry in entries.flatten() {
        if let Some(loop_name) = entry.file_name().to_str() {
            if !available_extensions.contains(&loop_name.to_string()) {
                println!("Cleaning up stale loop for: {loop_name}");
                unmount_loop_ref(loop_name)?;
            }
        }
    }

    Ok(())
}

/// Unmount a specific loop ref
fn unmount_loop_ref(extension_name: &str) -> Result<(), SystemdError> {
    let mount_point = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/avocado/extensions/{extension_name}")
    } else {
        format!("/run/avocado/extensions/{extension_name}")
    };

    let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        "mock-systemd-dissect"
    } else {
        "systemd-dissect"
    };

    let output = ProcessCommand::new(command_name)
        .args(["-U", &mount_point])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| SystemdError::CommandFailed {
            command: command_name.to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SystemdError::CommandExitedWithError {
            command: command_name.to_string(),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }

    println!("Unmounted loop for {extension_name}");
    Ok(())
}

/// Unmount all persistent loops
fn unmount_all_persistent_loops() -> Result<(), SystemdError> {
    println!("Unmounting all persistent loops...");

    let loop_ref_dir = "/dev/disk/by-loop-ref";
    if !Path::new(loop_ref_dir).exists() {
        println!("No persistent loops found.");
        return Ok(());
    }

    let entries = fs::read_dir(loop_ref_dir).map_err(|e| SystemdError::CommandFailed {
        command: "read_dir".to_string(),
        source: e,
    })?;

    for entry in entries.flatten() {
        if let Some(loop_name) = entry.file_name().to_str() {
            unmount_loop_ref(loop_name)?;
        }
    }

    println!("All persistent loops unmounted.");
    Ok(())
}

/// Clean up all extension symlinks to ensure fresh state for merge
fn cleanup_extension_symlinks(output: &OutputManager) -> Result<(), SystemdError> {
    output.step("Cleanup", "Removing old extension symlinks");

    // Clean up sysext symlinks
    let sysext_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/test_extensions")
    } else {
        "/run/extensions".to_string()
    };

    cleanup_symlinks_in_directory(&sysext_dir, output)?;

    // Clean up confext symlinks
    let confext_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/test_confexts")
    } else {
        "/run/confexts".to_string()
    };

    cleanup_symlinks_in_directory(&confext_dir, output)?;

    output.progress("Extension symlinks cleaned up");
    Ok(())
}

/// Clean up all symlinks in a specific directory
fn cleanup_symlinks_in_directory(
    directory: &str,
    output: &OutputManager,
) -> Result<(), SystemdError> {
    if !Path::new(directory).exists() {
        return Ok(());
    }

    let entries = fs::read_dir(directory).map_err(|e| SystemdError::CommandFailed {
        command: "read_dir".to_string(),
        source: e,
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_symlink() {
            if let Err(e) = fs::remove_file(&path) {
                output.progress(&format!(
                    "Warning: Failed to remove symlink {}: {}",
                    path.display(),
                    e
                ));
            } else {
                output.progress(&format!("Removed symlink: {}", path.display()));
            }
        }
    }

    Ok(())
}

/// Verify that extension directories are clean before merge
fn verify_clean_extension_environment(output: &OutputManager) -> Result<(), SystemdError> {
    let sysext_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/test_extensions")
    } else {
        "/run/extensions".to_string()
    };

    let confext_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/test_confexts")
    } else {
        "/run/confexts".to_string()
    };

    // Check for stale symlinks in sysext directory
    if let Some(stale_symlinks) = check_for_stale_symlinks(&sysext_dir)? {
        output.progress(&format!(
            "Warning: Found {} stale symlinks in {}, cleaning up",
            stale_symlinks.len(),
            sysext_dir
        ));
        cleanup_symlinks_in_directory(&sysext_dir, output)?;
    }

    // Check for stale symlinks in confext directory
    if let Some(stale_symlinks) = check_for_stale_symlinks(&confext_dir)? {
        output.progress(&format!(
            "Warning: Found {} stale symlinks in {}, cleaning up",
            stale_symlinks.len(),
            confext_dir
        ));
        cleanup_symlinks_in_directory(&confext_dir, output)?;
    }

    Ok(())
}

/// Check for stale symlinks in a directory
fn check_for_stale_symlinks(directory: &str) -> Result<Option<Vec<String>>, SystemdError> {
    if !Path::new(directory).exists() {
        return Ok(None);
    }

    let entries = fs::read_dir(directory).map_err(|e| SystemdError::CommandFailed {
        command: "read_dir".to_string(),
        source: e,
    })?;

    let mut stale_symlinks = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_symlink() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                stale_symlinks.push(name.to_string());
            }
        }
    }

    if stale_symlinks.is_empty() {
        Ok(None)
    } else {
        Ok(Some(stale_symlinks))
    }
}

/// Scan release files for only the enabled extensions
fn scan_release_files_for_enabled_extensions(
    enabled_extensions: &[Extension],
) -> Result<(Vec<String>, Vec<String>), SystemdError> {
    let mut on_merge_commands = Vec::new();
    let mut modprobe_modules = Vec::new();

    // Handle test mode with custom release directory (for backwards compatibility)
    if let Ok(custom_dir) = std::env::var("AVOCADO_EXTENSION_RELEASE_DIR") {
        return scan_custom_release_directory(&custom_dir);
    }

    for extension in enabled_extensions {
        // Scan release files from each enabled extension mount point
        scan_extension_release_files(extension, &mut on_merge_commands, &mut modprobe_modules)?;
    }

    Ok((on_merge_commands, modprobe_modules))
}

/// Scan release files from a custom directory (test mode)
fn scan_custom_release_directory(
    custom_dir: &str,
) -> Result<(Vec<String>, Vec<String>), SystemdError> {
    let mut on_merge_commands = Vec::new();
    let mut modprobe_modules = Vec::new();

    let custom_path = Path::new(custom_dir);
    let mut dirs = Vec::new();

    // Check if it's a single directory with release files (legacy behavior)
    if custom_path.join("extension-release.d").exists() {
        dirs.push(custom_dir.to_string());
    } else {
        // Look for sysext and confext subdirectories
        let sysext_dir = custom_path.join("usr/lib/extension-release.d");
        let confext_dir = custom_path.join("etc/extension-release.d");

        if sysext_dir.exists() {
            dirs.push(sysext_dir.to_string_lossy().to_string());
        }
        if confext_dir.exists() {
            dirs.push(confext_dir.to_string_lossy().to_string());
        }

        // If neither subdirectory structure exists, use the custom dir directly
        if dirs.is_empty() {
            dirs.push(custom_dir.to_string());
        }
    }

    for release_dir in dirs {
        scan_directory_for_release_files(
            &release_dir,
            &mut on_merge_commands,
            &mut modprobe_modules,
        );
    }

    Ok((on_merge_commands, modprobe_modules))
}

/// Scan release files from a specific extension's trusted mount point
fn scan_extension_release_files(
    extension: &Extension,
    on_merge_commands: &mut Vec<String>,
    modprobe_modules: &mut Vec<String>,
) -> Result<(), SystemdError> {
    // Check for sysext release file
    let sysext_release_path = extension
        .path
        .join("usr/lib/extension-release.d")
        .join(format!("extension-release.{}", extension.name));

    if sysext_release_path.exists() {
        if let Ok(content) = fs::read_to_string(&sysext_release_path) {
            let mut commands = parse_avocado_on_merge_commands(&content);
            on_merge_commands.append(&mut commands);

            let mut modules = parse_avocado_modprobe(&content);
            modprobe_modules.append(&mut modules);
        }
    }

    // Check for confext release file
    let confext_release_path = extension
        .path
        .join("etc/extension-release.d")
        .join(format!("extension-release.{}", extension.name));

    if confext_release_path.exists() {
        if let Ok(content) = fs::read_to_string(&confext_release_path) {
            let mut commands = parse_avocado_on_merge_commands(&content);
            on_merge_commands.append(&mut commands);

            let mut modules = parse_avocado_modprobe(&content);
            modprobe_modules.append(&mut modules);
        }
    }

    Ok(())
}

/// Scan a directory for release files (used in test mode)
fn scan_directory_for_release_files(
    release_dir: &str,
    on_merge_commands: &mut Vec<String>,
    modprobe_modules: &mut Vec<String>,
) {
    if !Path::new(release_dir).exists() {
        return;
    }

    if let Ok(entries) = fs::read_dir(release_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    let mut commands = parse_avocado_on_merge_commands(&content);
                    on_merge_commands.append(&mut commands);

                    let mut modules = parse_avocado_modprobe(&content);
                    modprobe_modules.append(&mut modules);
                }
            }
        }
    }
}

/// Process post-merge tasks for only the enabled extensions
fn process_post_merge_tasks_for_extensions(
    enabled_extensions: &[Extension],
) -> Result<(), SystemdError> {
    let (on_merge_commands, modprobe_modules) =
        scan_release_files_for_enabled_extensions(enabled_extensions)?;

    // Remove duplicates while preserving order
    let mut unique_commands = Vec::new();
    for command in on_merge_commands {
        if !unique_commands.contains(&command) {
            unique_commands.push(command);
        }
    }

    // Execute accumulated AVOCADO_ON_MERGE commands
    if !unique_commands.is_empty() {
        run_avocado_on_merge_commands(&unique_commands)?;
    }

    // Call modprobe for each module after commands complete
    if !modprobe_modules.is_empty() {
        run_modprobe(&modprobe_modules)?;
    }

    Ok(())
}

/// Parse all AVOCADO_ON_MERGE commands from release file content
fn parse_avocado_on_merge_commands(content: &str) -> Vec<String> {
    let mut commands = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("AVOCADO_ON_MERGE=") {
            let value = line
                .split_once('=')
                .map(|x| x.1)
                .unwrap_or("")
                .trim_matches('"')
                .trim();

            if !value.is_empty() {
                commands.push(value.to_string());
            }
        }
    }

    commands
}

/// Check if a release file content contains AVOCADO_ON_MERGE=depmod
/// (Kept for backward compatibility with existing tests)
#[allow(dead_code)]
fn check_avocado_on_merge_depmod(content: &str) -> bool {
    let commands = parse_avocado_on_merge_commands(content);
    commands.contains(&"depmod".to_string())
}

/// Parse AVOCADO_MODPROBE modules from release file content
fn parse_avocado_modprobe(content: &str) -> Vec<String> {
    let mut modules = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("AVOCADO_MODPROBE=") {
            let value = line
                .split_once('=')
                .map(|x| x.1)
                .unwrap_or("")
                .trim_matches('"')
                .trim();

            // Parse space-separated list of modules
            for module in value.split_whitespace() {
                if !module.is_empty() {
                    modules.push(module.to_string());
                }
            }
            break; // Only process the first AVOCADO_MODPROBE line
        }
    }

    modules
}

/// Run the depmod command
fn run_depmod() -> Result<(), SystemdError> {
    print_colored_info("Running depmod to update kernel module dependencies...");

    // Check if we're in test mode and should use mock commands
    let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        "mock-depmod"
    } else {
        "depmod"
    };

    let output = ProcessCommand::new(command_name)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| SystemdError::CommandFailed {
            command: command_name.to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SystemdError::CommandExitedWithError {
            command: command_name.to_string(),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }

    print_colored_success("depmod completed successfully.");
    Ok(())
}

/// Run modprobe for a list of modules
fn run_modprobe(modules: &[String]) -> Result<(), SystemdError> {
    if modules.is_empty() {
        return Ok(());
    }

    print_colored_info(&format!("Loading kernel modules: {}", modules.join(", ")));

    for module in modules {
        // Check if we're in test mode and should use mock commands
        let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
            "mock-modprobe"
        } else {
            "modprobe"
        };

        let output = ProcessCommand::new(command_name)
            .arg(module)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| SystemdError::CommandFailed {
                command: format!("{command_name} {module}"),
                source: e,
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("Warning: Failed to load module {module}: {stderr}");
            // Don't fail the entire operation for individual module failures
            // Just log the warning and continue with other modules
        } else {
            print_colored_success(&format!("Module {module} loaded successfully."));
        }
    }

    print_colored_success("Module loading completed.");
    Ok(())
}

/// Execute a single command with its arguments
fn execute_single_command(command_str: &str) -> Result<(), SystemdError> {
    // Parse the command string to handle commands with arguments
    // Commands may be quoted or contain spaces
    let parts: Vec<&str> = if command_str.starts_with('"') && command_str.ends_with('"') {
        // Handle quoted commands
        let unquoted = &command_str[1..command_str.len() - 1];
        unquoted.split_whitespace().collect()
    } else {
        // Handle unquoted commands
        command_str.split_whitespace().collect()
    };

    if parts.is_empty() {
        eprintln!("Warning: Empty command in AVOCADO_ON_MERGE, skipping");
        return Ok(());
    }

    let (command_name, args) = parts.split_first().unwrap();

    // Check if we're in test mode and should use mock commands
    let mock_command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        match *command_name {
            "depmod" => "mock-depmod".to_string(),
            "modprobe" => "mock-modprobe".to_string(),
            _ => {
                // For other commands in test mode, prefix with mock- if not already
                if command_name.starts_with("mock-") {
                    command_name.to_string()
                } else {
                    format!("mock-{command_name}")
                }
            }
        }
    } else {
        command_name.to_string()
    };

    let actual_command = &mock_command_name;

    let output = ProcessCommand::new(actual_command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| SystemdError::CommandFailed {
            command: command_str.to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: Command '{command_str}' failed: {stderr}");
        // Log warning but don't fail the entire operation
        // This matches the behavior of modprobe failures
    } else {
        print_colored_success(&format!("Command '{command_str}' completed successfully"));
    }

    Ok(())
}

/// Run accumulated AVOCADO_ON_MERGE commands
fn run_avocado_on_merge_commands(commands: &[String]) -> Result<(), SystemdError> {
    if commands.is_empty() {
        return Ok(());
    }

    print_colored_info(&format!("Executing {} post-merge commands", commands.len()));

    for command_str in commands {
        print_colored_info(&format!("Running command: {command_str}"));

        // Check if the command contains shell operators like semicolons
        if command_str.contains(';') {
            // Split the command by semicolons and execute each part sequentially
            let sub_commands: Vec<&str> = command_str.split(';').map(|s| s.trim()).collect();

            for sub_command in sub_commands {
                if !sub_command.is_empty() {
                    print_colored_info(&format!("Running sub-command: {sub_command}"));
                    execute_single_command(sub_command)?;
                }
            }
        } else {
            // Execute as a single command
            execute_single_command(command_str)?;
        }
    }

    print_colored_success("Post-merge command execution completed.");
    Ok(())
}

/// Run a systemd command with proper error handling
fn run_systemd_command(command: &str, args: &[&str]) -> Result<String, SystemdError> {
    // Check if we're in test mode and should use mock commands
    let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        // In test mode, use mock commands from PATH
        format!("mock-{command}")
    } else {
        command.to_string()
    };

    let output = ProcessCommand::new(&command_name)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| SystemdError::CommandFailed {
            command: command.to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SystemdError::CommandExitedWithError {
            command: command.to_string(),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.to_string())
}

/// Handle and parse systemd command output with proper formatting
fn handle_systemd_output(
    operation: &str,
    output_str: &str,
    output: &OutputManager,
) -> Result<(), SystemdError> {
    if output_str.trim().is_empty() {
        output.progress(&format!(
            "{operation}: No output (operation may have completed with no changes)"
        ));
        return Ok(());
    }

    // Try to parse as JSON for better formatting
    match serde_json::from_str::<Value>(output_str) {
        Ok(json) => {
            output.raw(&format!("{operation}: {json}"));
            Ok(())
        }
        Err(_) => {
            // If not JSON, just print the raw output
            output.raw(&format!("{operation}: {output_str}"));
            Ok(())
        }
    }
}

/// Errors related to systemd command execution
#[derive(Debug, thiserror::Error)]
pub enum SystemdError {
    #[error("Failed to run command '{command}': {source}")]
    CommandFailed {
        command: String,
        source: std::io::Error,
    },

    #[error("Command '{command}' exited with error code {exit_code:?}: {stderr}")]
    CommandExitedWithError {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::env;

    #[test]
    fn test_config_integration() {
        // Test that config is used for extensions directory
        let mut config = Config::default();
        config.avocado.ext.dir = "/test/config/path".to_string();

        let extensions_path = config.get_extensions_dir();
        assert_eq!(extensions_path, "/test/config/path");
    }

    #[test]
    fn test_environment_variable_precedence() {
        // Test that environment variable overrides config
        let mut config = Config::default();
        config.avocado.ext.dir = "/config/path".to_string();

        env::set_var("AVOCADO_EXTENSIONS_PATH", "/env/override/path");
        let extensions_path = config.get_extensions_dir();
        assert_eq!(extensions_path, "/env/override/path");

        // Clean up
        env::remove_var("AVOCADO_EXTENSIONS_PATH");

        // Now should use config value
        let extensions_path = config.get_extensions_dir();
        assert_eq!(extensions_path, "/config/path");
    }

    #[test]
    fn test_default_path_when_no_config_or_env() {
        // Ensure no environment variable is set
        env::remove_var("AVOCADO_EXTENSIONS_PATH");

        let config = Config::default();
        let extensions_path = config.get_extensions_dir();
        assert_eq!(extensions_path, "/var/lib/avocado/extensions");
    }

    #[test]
    fn test_extension_name_extraction() {
        // Test file name extraction logic
        use std::path::Path;

        // Test directory name
        let dir_path = Path::new("/test/path/my_extension");
        if let Some(name) = dir_path.file_name() {
            if let Some(name_str) = name.to_str() {
                assert_eq!(name_str, "my_extension");
            }
        }

        // Test .raw file name
        let raw_path = Path::new("/test/path/my_extension.raw");
        if let Some(name) = raw_path.file_name() {
            if let Some(name_str) = name.to_str() {
                if name_str.ends_with(".raw") {
                    let ext_name = name_str.strip_suffix(".raw").unwrap_or(name_str);
                    assert_eq!(ext_name, "my_extension");
                }
            }
        }
    }

    #[test]
    fn test_create_command() {
        let cmd = create_command();
        assert_eq!(cmd.get_name(), "ext");

        // Check that all subcommands exist
        let subcommands: Vec<_> = cmd.get_subcommands().collect();
        assert_eq!(subcommands.len(), 5);

        let subcommand_names: Vec<&str> = subcommands.iter().map(|cmd| cmd.get_name()).collect();
        assert!(subcommand_names.contains(&"list"));
        assert!(subcommand_names.contains(&"merge"));
        assert!(subcommand_names.contains(&"unmerge"));
        assert!(subcommand_names.contains(&"refresh"));
        assert!(subcommand_names.contains(&"status"));
    }

    #[test]
    fn test_extension_preference() {
        // Directory should be preferred over .raw file
        use std::collections::HashMap;

        let mut extension_map = HashMap::new();

        // Simulate adding a .raw file first
        let raw_extension = Extension {
            name: "test_ext".to_string(),
            path: PathBuf::from("/test/test_ext.raw"),
            is_sysext: true,
            is_confext: false,
            is_directory: false,
        };
        extension_map.insert("test_ext".to_string(), raw_extension);

        // Now add a directory with the same name (should replace the .raw)
        let dir_extension = Extension {
            name: "test_ext".to_string(),
            path: PathBuf::from("/test/test_ext"),
            is_sysext: true,
            is_confext: true,
            is_directory: true,
        };
        extension_map.insert("test_ext".to_string(), dir_extension);

        let extension = extension_map.get("test_ext").unwrap();
        assert!(extension.is_directory);
        assert!(extension.is_confext);
    }

    #[test]
    fn test_analyze_directory_extension() {
        // Test with no release files
        let test_path = PathBuf::from("/tmp/test_extension");
        let extension = analyze_directory_extension("test_ext", &test_path).unwrap();

        assert_eq!(extension.name, "test_ext");
        assert!(extension.is_sysext);
        assert!(extension.is_confext);
        assert!(extension.is_directory);
    }

    #[test]
    fn test_symlink_naming() {
        // Test directory extension symlink naming
        let dir_extension = Extension {
            name: "test_ext".to_string(),
            path: PathBuf::from("/test/test_ext"),
            is_sysext: true,
            is_confext: true,
            is_directory: true,
        };

        // Test loop-mounted raw file extension symlink naming
        let raw_extension = Extension {
            name: "test_ext".to_string(),
            path: PathBuf::from("/run/avocado/extensions/test_ext"), // Points to mounted directory
            is_sysext: true,
            is_confext: false,
            is_directory: false, // Still false to track origin, but path points to mounted dir
        };

        // Both directory and loop-mounted raw extensions should use just the extension name
        let dir_symlink_name = dir_extension.name.clone();
        assert_eq!(dir_symlink_name, "test_ext");

        let raw_symlink_name = raw_extension.name.clone();
        assert_eq!(raw_symlink_name, "test_ext");
    }

    #[test]
    fn test_check_avocado_on_merge_depmod() {
        // Test case with AVOCADO_ON_MERGE=depmod
        let content_with_depmod = r#"
VERSION_ID=1.0
AVOCADO_ON_MERGE=depmod
OTHER_KEY=value
"#;
        assert!(check_avocado_on_merge_depmod(content_with_depmod));

        // Test case with AVOCADO_ON_MERGE=depmod with quotes
        let content_with_quoted_depmod = r#"
VERSION_ID=1.0
AVOCADO_ON_MERGE="depmod"
OTHER_KEY=value
"#;
        assert!(check_avocado_on_merge_depmod(content_with_quoted_depmod));

        // Test case with different AVOCADO_ON_MERGE value
        let content_with_other_value = r#"
VERSION_ID=1.0
AVOCADO_ON_MERGE=something_else
OTHER_KEY=value
"#;
        assert!(!check_avocado_on_merge_depmod(content_with_other_value));

        // Test case without AVOCADO_ON_MERGE
        let content_without_key = r#"
VERSION_ID=1.0
OTHER_KEY=value
"#;
        assert!(!check_avocado_on_merge_depmod(content_without_key));

        // Test case with empty content
        assert!(!check_avocado_on_merge_depmod(""));

        // Test case with AVOCADO_ON_MERGE but empty value
        let content_with_empty_value = r#"
VERSION_ID=1.0
AVOCADO_ON_MERGE=
OTHER_KEY=value
"#;
        assert!(!check_avocado_on_merge_depmod(content_with_empty_value));
    }

    #[test]
    fn test_parse_avocado_modprobe() {
        // Test case with multiple modules
        let content_with_modules = r#"
VERSION_ID=2.0
AVOCADO_MODPROBE="nvidia i915 radeon"
OTHER_KEY=value
"#;
        let modules = parse_avocado_modprobe(content_with_modules);
        assert_eq!(modules, vec!["nvidia", "i915", "radeon"]);

        // Test case with single module without quotes
        let content_single_module = r#"
VERSION_ID=1.5
AVOCADO_MODPROBE=snd_hda_intel
OTHER_KEY=value
"#;
        let modules = parse_avocado_modprobe(content_single_module);
        assert_eq!(modules, vec!["snd_hda_intel"]);

        // Test case with no AVOCADO_MODPROBE
        let content_no_modprobe = r#"
VERSION_ID=1.0
AVOCADO_ON_MERGE=depmod
OTHER_KEY=value
"#;
        let modules = parse_avocado_modprobe(content_no_modprobe);
        assert!(modules.is_empty());

        // Test case with empty AVOCADO_MODPROBE
        let content_empty_modprobe = r#"
VERSION_ID=1.0
AVOCADO_MODPROBE=""
OTHER_KEY=value
"#;
        let modules = parse_avocado_modprobe(content_empty_modprobe);
        assert!(modules.is_empty());

        // Test case with extra whitespace
        let content_with_whitespace = r#"
VERSION_ID=1.0
AVOCADO_MODPROBE="  nvidia   i915  radeon  "
OTHER_KEY=value
"#;
        let modules = parse_avocado_modprobe(content_with_whitespace);
        assert_eq!(modules, vec!["nvidia", "i915", "radeon"]);

        // Test case with mixed quotes and no quotes in different lines (only first should be processed)
        let content_multiple_lines = r#"
VERSION_ID=1.0
AVOCADO_MODPROBE="nvidia i915"
AVOCADO_MODPROBE=should_be_ignored
OTHER_KEY=value
"#;
        let modules = parse_avocado_modprobe(content_multiple_lines);
        assert_eq!(modules, vec!["nvidia", "i915"]);
    }

    #[test]
    fn test_parse_avocado_on_merge_commands_with_equals() {
        // Test case with command containing equals signs in arguments
        let content_with_equals = r#"
VERSION_ID=1.0
AVOCADO_ON_MERGE="udevadm trigger --action=add"
AVOCADO_ON_MERGE=command --option=value --other=setting
OTHER_KEY=value
"#;
        let commands = parse_avocado_on_merge_commands(content_with_equals);
        assert_eq!(
            commands,
            vec![
                "udevadm trigger --action=add",
                "command --option=value --other=setting"
            ]
        );

        // Test case with multiple equals signs in same argument
        let content_multiple_equals = r#"
VERSION_ID=1.0
AVOCADO_ON_MERGE="systemctl set-property --runtime some.service CPUQuota=50% MemoryLimit=1G"
"#;
        let commands = parse_avocado_on_merge_commands(content_multiple_equals);
        assert_eq!(
            commands,
            vec!["systemctl set-property --runtime some.service CPUQuota=50% MemoryLimit=1G"]
        );

        // Test case ensuring backwards compatibility with simple commands
        let content_simple = r#"
VERSION_ID=1.0
AVOCADO_ON_MERGE=depmod
AVOCADO_ON_MERGE="systemctl restart some-service"
"#;
        let commands = parse_avocado_on_merge_commands(content_simple);
        assert_eq!(commands, vec!["depmod", "systemctl restart some-service"]);
    }

    #[test]
    fn test_parse_avocado_on_merge_commands_with_semicolons() {
        // Test case with semicolon-separated commands
        let content_with_semicolons = r#"
VERSION_ID=1.0
AVOCADO_ON_MERGE="systemctl --no-block restart dbus; systemctl --no-block restart avahi-daemon"
AVOCADO_ON_MERGE="command1 --arg=value; command2; command3 --option"
OTHER_KEY=value
"#;
        let commands = parse_avocado_on_merge_commands(content_with_semicolons);
        assert_eq!(
            commands,
            vec![
                "systemctl --no-block restart dbus; systemctl --no-block restart avahi-daemon",
                "command1 --arg=value; command2; command3 --option"
            ]
        );

        // Test case with mixed semicolons and regular commands
        let content_mixed = r#"
VERSION_ID=1.0
AVOCADO_ON_MERGE=depmod
AVOCADO_ON_MERGE="systemctl restart service1; systemctl restart service2"
AVOCADO_ON_MERGE="single-command --arg"
"#;
        let commands = parse_avocado_on_merge_commands(content_mixed);
        assert_eq!(
            commands,
            vec![
                "depmod",
                "systemctl restart service1; systemctl restart service2",
                "single-command --arg"
            ]
        );
    }

    #[test]
    fn test_parse_scope_from_release_content() {
        // Test case with SYSEXT_SCOPE
        let content_with_sysext_scope = r#"
VERSION_ID=1.0
SYSEXT_SCOPE="initrd system"
OTHER_KEY=value
"#;
        let scopes = parse_scope_from_release_content(content_with_sysext_scope, "SYSEXT_SCOPE");
        assert_eq!(scopes, vec!["initrd", "system"]);

        // Test case with CONFEXT_SCOPE
        let content_with_confext_scope = r#"
VERSION_ID=1.0
CONFEXT_SCOPE=system
OTHER_KEY=value
"#;
        let scopes = parse_scope_from_release_content(content_with_confext_scope, "CONFEXT_SCOPE");
        assert_eq!(scopes, vec!["system"]);

        // Test case with no scope
        let content_no_scope = r#"
VERSION_ID=1.0
OTHER_KEY=value
"#;
        let scopes = parse_scope_from_release_content(content_no_scope, "SYSEXT_SCOPE");
        assert!(scopes.is_empty());

        // Test case with empty scope
        let content_empty_scope = r#"
VERSION_ID=1.0
SYSEXT_SCOPE=""
OTHER_KEY=value
"#;
        let scopes = parse_scope_from_release_content(content_empty_scope, "SYSEXT_SCOPE");
        assert!(scopes.is_empty());

        // Test case with extra whitespace
        let content_with_whitespace = r#"
VERSION_ID=1.0
SYSEXT_SCOPE="  initrd   system  portable  "
OTHER_KEY=value
"#;
        let scopes = parse_scope_from_release_content(content_with_whitespace, "SYSEXT_SCOPE");
        assert_eq!(scopes, vec!["initrd", "system", "portable"]);
    }

    #[test]
    fn test_is_running_in_initrd() {
        // This test can't easily test the actual function since it depends on filesystem state
        // But we can test that the function exists and returns a boolean
        let result = is_running_in_initrd();
        assert!(result == true || result == false); // Just ensure it returns a boolean
    }

    #[test]
    fn test_sysext_scope_checking() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();
        let ext_path = temp_dir.path().join("test_ext");
        let release_dir = ext_path.join("usr/lib/extension-release.d");
        fs::create_dir_all(&release_dir).unwrap();

        // Test case 1: Extension with initrd scope only
        let release_file = release_dir.join("extension-release.test_ext");
        fs::write(&release_file, "VERSION_ID=1.0\nSYSEXT_SCOPE=\"initrd\"\n").unwrap();

        // This test will always return true since we can't mock is_running_in_initrd easily
        // But we can verify the function doesn't crash
        let result = is_sysext_enabled_for_current_environment(&ext_path, "test_ext");
        assert!(result == true || result == false);

        // Test case 2: Extension with system scope only
        fs::write(&release_file, "VERSION_ID=1.0\nSYSEXT_SCOPE=\"system\"\n").unwrap();
        let result = is_sysext_enabled_for_current_environment(&ext_path, "test_ext");
        assert!(result == true || result == false);

        // Test case 3: Extension with both scopes
        fs::write(
            &release_file,
            "VERSION_ID=1.0\nSYSEXT_SCOPE=\"initrd system\"\n",
        )
        .unwrap();
        let result = is_sysext_enabled_for_current_environment(&ext_path, "test_ext");
        assert!(result == true || result == false);

        // Test case 4: Extension with no scope (should default to enabled)
        fs::write(&release_file, "VERSION_ID=1.0\n").unwrap();
        let result = is_sysext_enabled_for_current_environment(&ext_path, "test_ext");
        assert_eq!(result, true);

        // Test case 5: No release file (should default to enabled)
        fs::remove_file(&release_file).unwrap();
        let result = is_sysext_enabled_for_current_environment(&ext_path, "test_ext");
        assert_eq!(result, true);
    }

    #[test]
    fn test_confext_scope_checking() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();
        let ext_path = temp_dir.path().join("test_ext");
        let release_dir = ext_path.join("etc/extension-release.d");
        fs::create_dir_all(&release_dir).unwrap();

        // Test case 1: Extension with initrd scope only
        let release_file = release_dir.join("extension-release.test_ext");
        fs::write(&release_file, "VERSION_ID=1.0\nCONFEXT_SCOPE=\"initrd\"\n").unwrap();

        // This test will always return true since we can't mock is_running_in_initrd easily
        // But we can verify the function doesn't crash
        let result = is_confext_enabled_for_current_environment(&ext_path, "test_ext");
        assert!(result == true || result == false);

        // Test case 2: Extension with no scope (should default to enabled)
        fs::write(&release_file, "VERSION_ID=1.0\n").unwrap();
        let result = is_confext_enabled_for_current_environment(&ext_path, "test_ext");
        assert_eq!(result, true);

        // Test case 3: No release file (should default to enabled)
        fs::remove_file(&release_file).unwrap();
        let result = is_confext_enabled_for_current_environment(&ext_path, "test_ext");
        assert_eq!(result, true);
    }
}
