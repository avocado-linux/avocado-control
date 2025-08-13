use crate::config::Config;
use clap::{ArgMatches, Command};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::os::unix::fs as unix_fs;

/// Represents an extension and its type(s)
#[derive(Debug, Clone)]
struct Extension {
    name: String,
    path: PathBuf,
    is_sysext: bool,
    is_confext: bool,
    is_directory: bool, // true for directories, false for .raw files
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
                .about("Unmerge extensions using systemd-sysext and systemd-confext"),
        )
        .subcommand(
            Command::new("refresh").about("Unmerge and then merge extensions (refresh extensions)"),
        )
        .subcommand(Command::new("status").about("Show status of merged extensions"))
}

/// Handle ext command and its subcommands
pub fn handle_command(matches: &ArgMatches, config: &Config) {
    match matches.subcommand() {
        Some(("list", _)) => {
            list_extensions(config);
        }
        Some(("merge", _)) => {
            merge_extensions();
        }
        Some(("unmerge", _)) => {
            unmerge_extensions();
        }
        Some(("refresh", _)) => {
            refresh_extensions();
        }
        Some(("status", _)) => {
            status_extensions();
        }
        _ => {
            println!("Use 'avocadoctl ext --help' for available extension commands");
        }
    }
}

/// List all extensions from the extensions directory
fn list_extensions(config: &Config) {
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
fn merge_extensions() {
    match merge_extensions_internal() {
        Ok(_) => println!("Extensions merged successfully."),
        Err(e) => {
            eprintln!("Failed to merge extensions: {e}");
            std::process::exit(1);
        }
    }
}

/// Internal merge function that returns a Result for use in remerge
fn merge_extensions_internal() -> Result<(), SystemdError> {
    println!("Merging extensions...");

    // Prepare the environment by setting up symlinks
    prepare_extension_environment()?;

    // Merge system extensions
    let output = run_systemd_command(
        "systemd-sysext",
        &["merge", "--mutable=ephemeral", "--json=short"],
    )?;
    handle_systemd_output("systemd-sysext merge", &output)?;

    // Merge configuration extensions
    let output = run_systemd_command(
        "systemd-confext",
        &["merge", "--mutable=ephemeral", "--json=short"],
    )?;
    handle_systemd_output("systemd-confext merge", &output)?;

    // Process post-merge tasks
    process_post_merge_tasks()?;

    Ok(())
}

/// Unmerge extensions using systemd-sysext and systemd-confext
fn unmerge_extensions() {
    match unmerge_extensions_internal() {
        Ok(_) => println!("Extensions unmerged successfully."),
        Err(e) => {
            eprintln!("Failed to unmerge extensions: {e}");
            std::process::exit(1);
        }
    }
}

/// Internal unmerge function that returns a Result for use in refresh
fn unmerge_extensions_internal() -> Result<(), SystemdError> {
    unmerge_extensions_internal_with_depmod(true)
}

/// Internal unmerge function with optional depmod control
fn unmerge_extensions_internal_with_depmod(call_depmod: bool) -> Result<(), SystemdError> {
    println!("Unmerging extensions...");

    // Unmerge system extensions
    let output = run_systemd_command("systemd-sysext", &["unmerge", "--json=short"])?;
    handle_systemd_output("systemd-sysext unmerge", &output)?;

    // Unmerge configuration extensions
    let output = run_systemd_command("systemd-confext", &["unmerge", "--json=short"])?;
    handle_systemd_output("systemd-confext unmerge", &output)?;

    // Run depmod after unmerge if requested
    if call_depmod {
        run_depmod()?;
    }

    Ok(())
}

/// Refresh extensions (unmerge then merge)
fn refresh_extensions() {
    println!("Refreshing extensions (unmerge then merge)...");

    // First unmerge (skip depmod since we'll call it after merge)
    if let Err(e) = unmerge_extensions_internal_with_depmod(false) {
        eprintln!("Failed to unmerge extensions: {e}");
        std::process::exit(1);
    }
    println!("Extensions unmerged successfully.");

    // Then merge (this will call depmod via post-merge processing)
    if let Err(e) = merge_extensions_internal() {
        eprintln!("Failed to merge extensions: {e}");
        std::process::exit(1);
    }
    println!("Extensions merged successfully.");

    println!("Extensions refreshed successfully.");
}

/// Show status of merged extensions
fn status_extensions() {
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

/// Prepare the extension environment by setting up symlinks
fn prepare_extension_environment() -> Result<(), SystemdError> {
    println!("Preparing extension environment...");

    // Use default extensions directory path
    let extensions_dir = std::env::var("AVOCADO_EXTENSIONS_PATH")
        .unwrap_or_else(|_| "/var/lib/avocado/extensions".to_string());

    // Scan for available extensions
    let extensions = scan_extensions(&extensions_dir)?;

    if extensions.is_empty() {
        println!("No extensions found in {}", extensions_dir);
        return Ok(());
    }

    // Create target directories
    create_target_directories()?;

    // Create symlinks for sysext and confext extensions
    for extension in &extensions {
        if extension.is_sysext {
            create_sysext_symlink(extension)?;
        }
        if extension.is_confext {
            create_confext_symlink(extension)?;
        }
    }

    println!("Extension environment prepared successfully.");
    Ok(())
}

/// Scan the extensions directory for available extensions
fn scan_extensions(extensions_dir: &str) -> Result<Vec<Extension>, SystemdError> {
    let mut extensions = Vec::new();
    let mut extension_map = std::collections::HashMap::new();

    // Check if extensions directory exists
    if !Path::new(extensions_dir).exists() {
        return Ok(extensions);
    }

    // Read all entries in the extensions directory
    let entries = fs::read_dir(extensions_dir).map_err(|e| SystemdError::CommandFailed {
        command: "scan_extensions".to_string(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| SystemdError::CommandFailed {
            command: "scan_extensions".to_string(),
            source: e,
        })?;

        let path = entry.path();

        if let Some(file_name) = path.file_name() {
            if let Some(name_str) = file_name.to_str() {
                if path.is_dir() {
                    // It's a directory extension
                    let extension = analyze_directory_extension(name_str, &path)?;
                    extension_map.insert(name_str.to_string(), extension);
                } else if name_str.ends_with(".raw") {
                    // It's a .raw file extension
                    let ext_name = name_str.strip_suffix(".raw").unwrap_or(name_str);

                    // Only add if we don't already have a directory with the same name
                    if !extension_map.contains_key(ext_name) {
                        let extension = analyze_raw_extension(ext_name, &path)?;
                        extension_map.insert(ext_name.to_string(), extension);
                    }
                }
            }
        }
    }

    // Convert map to vector
    extensions.extend(extension_map.into_values());
    Ok(extensions)
}

/// Analyze a directory extension to determine if it's sysext, confext, or both
fn analyze_directory_extension(name: &str, path: &PathBuf) -> Result<Extension, SystemdError> {
    let mut is_sysext = false;
    let mut is_confext = false;

    // Look for extension-release files
    let sysext_release_path = path.join("usr/lib/extension-release.d").join(format!("extension-release.{}", name));
    let confext_release_path = path.join("etc/extension-release.d").join(format!("extension-release.{}", name));

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

    Ok(Extension {
        name: name.to_string(),
        path: path.clone(),
        is_sysext,
        is_confext,
        is_directory: true,
    })
}

/// Analyze a .raw file extension using systemd-dissect
fn analyze_raw_extension(name: &str, path: &PathBuf) -> Result<Extension, SystemdError> {
    println!("Analyzing raw extension: {}", name);

    // Check if we're in test mode and should use mock commands
    let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        "mock-systemd-dissect"
    } else {
        "systemd-dissect"
    };

    let output = ProcessCommand::new(command_name)
        .args(&["--json=short", path.to_str().unwrap_or("")])
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

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    let json: Value = serde_json::from_str(&stdout).map_err(|e| SystemdError::CommandFailed {
        command: "parse_json".to_string(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
    })?;

    let is_sysext = json.get("useSystemExtension")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let is_confext = json.get("useConfigurationExtension")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(Extension {
        name: name.to_string(),
        path: path.clone(),
        is_sysext,
        is_confext,
        is_directory: false,
    })
}

/// Create target directories for symlinks
fn create_target_directories() -> Result<(), SystemdError> {
    let (sysext_dir, confext_dir) = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        // In test mode, use temporary directories
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        (
            format!("{}/test_extensions", temp_base),
            format!("{}/test_confexts", temp_base),
        )
    } else {
        (
            "/run/extensions".to_string(),
            "/run/confexts".to_string(),
        )
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

/// Create a symlink for a sysext extension
fn create_sysext_symlink(extension: &Extension) -> Result<(), SystemdError> {
    let sysext_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{}/test_extensions", temp_base)
    } else {
        "/run/extensions".to_string()
    };

    // Use the original filename for .raw files, extension name for directories
    let symlink_name = if extension.is_directory {
        extension.name.clone()
    } else {
        // For .raw files, use the original filename with extension
        format!("{}.raw", extension.name)
    };

    let target_path = format!("{}/{}", sysext_dir, symlink_name);

    // Remove existing symlink or file if it exists
    if Path::new(&target_path).exists() {
        let path = Path::new(&target_path);

        // Try to remove as file first (works for symlinks and regular files)
        if let Err(_) = fs::remove_file(&target_path) {
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

    println!("Created sysext symlink: {} -> {}", target_path, extension.path.display());
    Ok(())
}

/// Create a symlink for a confext extension
fn create_confext_symlink(extension: &Extension) -> Result<(), SystemdError> {
    let confext_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{}/test_confexts", temp_base)
    } else {
        "/run/confexts".to_string()
    };

    // Use the original filename for .raw files, extension name for directories
    let symlink_name = if extension.is_directory {
        extension.name.clone()
    } else {
        // For .raw files, use the original filename with extension
        format!("{}.raw", extension.name)
    };

    let target_path = format!("{}/{}", confext_dir, symlink_name);

    // Remove existing symlink or file if it exists
    if Path::new(&target_path).exists() {
        let path = Path::new(&target_path);

        // Try to remove as file first (works for symlinks and regular files)
        if let Err(_) = fs::remove_file(&target_path) {
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

    println!("Created confext symlink: {} -> {}", target_path, extension.path.display());
    Ok(())
}

/// Process post-merge tasks by checking extension release files
fn process_post_merge_tasks() -> Result<(), SystemdError> {
    let release_dir = std::env::var("AVOCADO_EXTENSION_RELEASE_DIR")
        .unwrap_or_else(|_| "/usr/lib/extension-release.d".to_string());

    // Check if the release directory exists
    if !Path::new(&release_dir).exists() {
        // This is not an error - just means no extensions are merged or old systemd version
        return Ok(());
    }

    let mut depmod_needed = false;

    // Read all files in the extension release directory
    match fs::read_dir(&release_dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if check_avocado_on_merge_depmod(&content) {
                            depmod_needed = true;
                            // We can break early since we only need to call depmod once
                            break;
                        }
                    }
                }
            }
        }
        Err(e) => {
            // Log the error but don't fail the entire operation
            eprintln!("Warning: Could not read extension release directory {release_dir}: {e}");
            return Ok(());
        }
    }

    // Call depmod if needed
    if depmod_needed {
        run_depmod()?;
    }

    Ok(())
}

/// Check if a release file content contains AVOCADO_ON_MERGE=depmod
fn check_avocado_on_merge_depmod(content: &str) -> bool {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("AVOCADO_ON_MERGE=") {
            let value = line
                .split('=')
                .nth(1)
                .unwrap_or("")
                .trim_matches('"')
                .trim();
            if value == "depmod" {
                return true;
            }
        }
    }
    false
}

/// Run the depmod command
fn run_depmod() -> Result<(), SystemdError> {
    println!("Running depmod to update kernel module dependencies...");

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

    println!("depmod completed successfully.");
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

/// Handle and parse systemd command output
fn handle_systemd_output(operation: &str, output: &str) -> Result<(), SystemdError> {
    if output.trim().is_empty() {
        println!("{operation}: No output (operation may have completed with no changes)");
        return Ok(());
    }

    // Try to parse as JSON
    match serde_json::from_str::<Value>(output) {
        Ok(json) => {
            println!("{operation}: {json}");
            Ok(())
        }
        Err(_) => {
            // If not JSON, just print the raw output
            println!("{operation}: {output}");
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

        // Test raw file extension symlink naming
        let raw_extension = Extension {
            name: "test_ext".to_string(),
            path: PathBuf::from("/test/test_ext.raw"),
            is_sysext: true,
            is_confext: false,
            is_directory: false,
        };

        // For directories, symlink name should be just the extension name
        let dir_symlink_name = if dir_extension.is_directory {
            dir_extension.name.clone()
        } else {
            format!("{}.raw", dir_extension.name)
        };
        assert_eq!(dir_symlink_name, "test_ext");

        // For .raw files, symlink name should include .raw extension
        let raw_symlink_name = if raw_extension.is_directory {
            raw_extension.name.clone()
        } else {
            format!("{}.raw", raw_extension.name)
        };
        assert_eq!(raw_symlink_name, "test_ext.raw");
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
}
