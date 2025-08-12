use crate::config::Config;
use clap::{ArgMatches, Command};
use std::fs;
use std::process::{Command as ProcessCommand, Stdio};
use serde_json::Value;

/// Create the ext subcommand definition
pub fn create_command() -> Command {
    Command::new("ext")
        .about("Extension management commands")
        .subcommand(Command::new("list").about("List all available extensions"))
        .subcommand(
            Command::new("merge")
                .about("Merge extensions using systemd-sysext and systemd-confext")
        )
        .subcommand(
            Command::new("unmerge")
                .about("Unmerge extensions using systemd-sysext and systemd-confext")
        )
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
    println!("Merging extensions...");

    let mut success = true;

    // Merge system extensions
    match run_systemd_command("systemd-sysext", &["merge", "--mutable=ephemeral", "--json=short"]) {
        Ok(output) => {
            if let Err(e) = handle_systemd_output("systemd-sysext merge", &output) {
                eprintln!("Error processing systemd-sysext output: {e}");
                success = false;
            }
        }
        Err(e) => {
            eprintln!("Error running systemd-sysext merge: {e}");
            success = false;
        }
    }

    // Merge configuration extensions
    match run_systemd_command("systemd-confext", &["merge", "--mutable=ephemeral", "--json=short"]) {
        Ok(output) => {
            if let Err(e) = handle_systemd_output("systemd-confext merge", &output) {
                eprintln!("Error processing systemd-confext output: {e}");
                success = false;
            }
        }
        Err(e) => {
            eprintln!("Error running systemd-confext merge: {e}");
            success = false;
        }
    }

    if success {
        println!("Extensions merged successfully.");
    } else {
        std::process::exit(1);
    }
}

/// Unmerge extensions using systemd-sysext and systemd-confext
fn unmerge_extensions() {
    println!("Unmerging extensions...");

    let mut success = true;

    // Unmerge system extensions
    match run_systemd_command("systemd-sysext", &["unmerge", "--json=short"]) {
        Ok(output) => {
            if let Err(e) = handle_systemd_output("systemd-sysext unmerge", &output) {
                eprintln!("Error processing systemd-sysext output: {e}");
                success = false;
            }
        }
        Err(e) => {
            eprintln!("Error running systemd-sysext unmerge: {e}");
            success = false;
        }
    }

    // Unmerge configuration extensions
    match run_systemd_command("systemd-confext", &["unmerge", "--json=short"]) {
        Ok(output) => {
            if let Err(e) = handle_systemd_output("systemd-confext unmerge", &output) {
                eprintln!("Error processing systemd-confext output: {e}");
                success = false;
            }
        }
        Err(e) => {
            eprintln!("Error running systemd-confext unmerge: {e}");
            success = false;
        }
    }

    if success {
        println!("Extensions unmerged successfully.");
    } else {
        std::process::exit(1);
    }
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
            println!("{operation}: {}", json);
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
        assert_eq!(subcommands.len(), 3);

        let subcommand_names: Vec<&str> = subcommands.iter().map(|cmd| cmd.get_name()).collect();
        assert!(subcommand_names.contains(&"list"));
        assert!(subcommand_names.contains(&"merge"));
        assert!(subcommand_names.contains(&"unmerge"));
    }
}
