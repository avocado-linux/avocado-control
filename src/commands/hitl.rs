use crate::commands::ext;
use clap::{Arg, ArgMatches, Command};
use std::fs;
use std::path::Path;
use std::process::{Command as ProcessCommand, Stdio};

/// Create the hitl subcommand definition
pub fn create_command() -> Command {
    Command::new("hitl")
        .about("Hardware-in-the-loop (HITL) testing commands")
        .subcommand(
            Command::new("mount")
                .about("Mount NFS extensions from a remote server")
                .arg(
                    Arg::new("server-ip")
                        .short('s')
                        .long("server-ip")
                        .value_name("IP")
                        .help("Server IP address")
                        .required(true),
                )
                .arg(
                    Arg::new("server-port")
                        .short('p')
                        .long("server-port")
                        .value_name("PORT")
                        .help("Server port number")
                        .default_value("12049"),
                )
                .arg(
                    Arg::new("extension")
                        .short('e')
                        .long("extension")
                        .value_name("NAME")
                        .help("Extension name to mount (can be specified multiple times)")
                        .action(clap::ArgAction::Append)
                        .required(true),
                )
                .arg(
                    Arg::new("verbose")
                        .short('v')
                        .long("verbose")
                        .help("Show detailed output during mount operation")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
}

/// Handle hitl command and its subcommands
pub fn handle_command(matches: &ArgMatches) {
    match matches.subcommand() {
        Some(("mount", mount_matches)) => {
            let verbose = mount_matches.get_flag("verbose");
            mount_extensions_with_verbosity(mount_matches, verbose);
        }
        _ => {
            println!("Use 'avocadoctl hitl --help' for available HITL commands");
        }
    }
}

/// Mount NFS extensions from a remote server (legacy)
fn mount_extensions(matches: &ArgMatches) {
    mount_extensions_with_verbosity(matches, false);
}

/// Mount NFS extensions from a remote server with verbosity control
fn mount_extensions_with_verbosity(matches: &ArgMatches, verbose: bool) {
    let server_ip = matches
        .get_one::<String>("server-ip")
        .expect("server-ip is required");
    let server_port = matches
        .get_one::<String>("server-port")
        .expect("server-port has default value");
    let extensions: Vec<&String> = matches
        .get_many::<String>("extension")
        .expect("at least one extension is required")
        .collect();

    if verbose {
        println!("Mounting HITL extensions from {server_ip}:{server_port}");
    } else {
        println!("Mounting HITL extensions...");
    }

    let extensions_base_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/avocado/hitl")
    } else {
        "/run/avocado/hitl".to_string()
    };
    let mut success = true;

    for extension in &extensions {
        if verbose {
            println!("Setting up extension: {extension}");
        }

        // Create extension directory
        let extension_dir = format!("{extensions_base_dir}/{extension}");
        if let Err(e) = create_extension_directory_with_verbosity(&extension_dir, verbose) {
            eprintln!("Failed to create directory {extension_dir}: {e}");
            success = false;
            continue;
        }

        // Mount NFS share
        if let Err(e) = mount_nfs_extension_with_verbosity(server_ip, server_port, extension, &extension_dir, verbose) {
            eprintln!("Failed to mount extension {extension}: {e}");
            success = false;
            continue;
        }

        if verbose {
            println!("Successfully mounted extension: {extension}");
        }
    }

    if success {
        if verbose {
            println!("All extensions mounted successfully.");
            println!("Refreshing extensions to apply mounted changes...");
            ext::refresh_extensions_verbose();
        } else {
            println!("Extensions mounted.");
            ext::refresh_extensions();
        }
    } else {
        eprintln!("Some extensions failed to mount.");
        std::process::exit(1);
    }
}

/// Create extension directory with proper error handling (legacy)
fn create_extension_directory(dir_path: &str) -> Result<(), std::io::Error> {
    create_extension_directory_with_verbosity(dir_path, true)
}

/// Create extension directory with proper error handling and verbosity control
fn create_extension_directory_with_verbosity(dir_path: &str, verbose: bool) -> Result<(), std::io::Error> {
    if !Path::new(dir_path).exists() {
        fs::create_dir_all(dir_path)?;
        if verbose {
            println!("Created directory: {dir_path}");
        }
    } else if verbose {
        println!("Directory already exists: {dir_path}");
    }
    Ok(())
}

/// Mount NFS extension with proper error handling (legacy)
fn mount_nfs_extension(
    server_ip: &str,
    server_port: &str,
    extension: &str,
    mount_point: &str,
) -> Result<(), HitlError> {
    mount_nfs_extension_with_verbosity(server_ip, server_port, extension, mount_point, true)
}

/// Mount NFS extension with proper error handling and verbosity control
fn mount_nfs_extension_with_verbosity(
    server_ip: &str,
    server_port: &str,
    extension: &str,
    mount_point: &str,
    verbose: bool,
) -> Result<(), HitlError> {
    let nfs_source = format!("{server_ip}:/{extension}");
    let mount_options = format!("port={server_port},vers=4,hard,timeo=600,retrans=2,acregmin=0,acregmax=1,acdirmin=0,acdirmax=1,lookupcache=none");

    if verbose {
        println!("Mounting {nfs_source} to {mount_point} with options: {mount_options}");
    }

    // Check if we're in test mode and should use mock commands
    let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        "mock-mount"
    } else {
        "mount"
    };

    let output = ProcessCommand::new(command_name)
        .args(["-t", "nfs4", "-o", &mount_options, &nfs_source, mount_point])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| HitlError::CommandFailed {
            command: command_name.to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HitlError::MountFailed {
            extension: extension.to_string(),
            mount_point: mount_point.to_string(),
            error: stderr.to_string(),
        });
    }

    Ok(())
}

/// Errors related to HITL operations
#[derive(Debug, thiserror::Error)]
pub enum HitlError {
    #[error("Failed to run command '{command}': {source}")]
    CommandFailed {
        command: String,
        source: std::io::Error,
    },

    #[error("Failed to mount extension '{extension}' to '{mount_point}': {error}")]
    MountFailed {
        extension: String,
        mount_point: String,
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_command() {
        let cmd = create_command();
        assert_eq!(cmd.get_name(), "hitl");

        // Check that mount subcommand exists
        let subcommands: Vec<_> = cmd.get_subcommands().collect();
        assert_eq!(subcommands.len(), 1);

        let subcommand_names: Vec<&str> = subcommands.iter().map(|cmd| cmd.get_name()).collect();
        assert!(subcommand_names.contains(&"mount"));
    }

    #[test]
    fn test_mount_command_args() {
        let cmd = create_command();
        let mount_cmd = cmd
            .get_subcommands()
            .find(|subcmd| subcmd.get_name() == "mount")
            .expect("mount subcommand should exist");

        // Check required arguments
        let args: Vec<_> = mount_cmd.get_arguments().collect();
        let arg_names: Vec<&str> = args.iter().map(|arg| arg.get_id().as_str()).collect();

        assert!(arg_names.contains(&"server-ip"));
        assert!(arg_names.contains(&"server-port"));
        assert!(arg_names.contains(&"extension"));
    }
}
