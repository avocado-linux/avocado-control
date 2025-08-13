use crate::commands::ext;
use crate::output::OutputManager;
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
                ),
        )
        .subcommand(
            Command::new("unmount").about("Unmount NFS extensions").arg(
                Arg::new("extension")
                    .short('e')
                    .long("extension")
                    .value_name("NAME")
                    .help("Extension name to unmount (can be specified multiple times)")
                    .action(clap::ArgAction::Append)
                    .required(true),
            ),
        )
}

/// Handle hitl command and its subcommands
pub fn handle_command(matches: &ArgMatches, output: &OutputManager) {
    match matches.subcommand() {
        Some(("mount", mount_matches)) => {
            mount_extensions(mount_matches, output);
        }
        Some(("unmount", unmount_matches)) => {
            unmount_extensions(unmount_matches, output);
        }
        _ => {
            println!("Use 'avocadoctl hitl --help' for available HITL commands");
        }
    }
}

/// Mount NFS extensions from a remote server
fn mount_extensions(matches: &ArgMatches, output: &OutputManager) {
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

    output.info(
        "HITL Mount",
        &format!("Mounting extensions from {server_ip}:{server_port}"),
    );

    let extensions_base_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/avocado/hitl")
    } else {
        "/run/avocado/hitl".to_string()
    };
    let mut success = true;

    for extension in &extensions {
        output.step("HITL Mount", &format!("Setting up extension: {extension}"));

        // Create extension directory
        let extension_dir = format!("{extensions_base_dir}/{extension}");
        if let Err(e) = create_extension_directory(&extension_dir, output) {
            output.error(
                "HITL Mount",
                &format!("Failed to create directory {extension_dir}: {e}"),
            );
            success = false;
            continue;
        }

        // Mount NFS share
        if let Err(e) =
            mount_nfs_extension(server_ip, server_port, extension, &extension_dir, output)
        {
            output.error(
                "HITL Mount",
                &format!("Failed to mount extension {extension}: {e}"),
            );

            // Clean up the directory that was created since the mount failed
            if let Err(cleanup_err) = cleanup_extension_directory(&extension_dir, output) {
                output.error(
                    "HITL Mount",
                    &format!("Failed to cleanup directory for {extension}: {cleanup_err}"),
                );
            }

            success = false;
            continue;
        }

        output.progress(&format!("Successfully mounted extension: {extension}"));
    }

    if success {
        output.success("HITL Mount", "All extensions mounted successfully");
        output.info(
            "HITL Mount",
            "Refreshing extensions to apply mounted changes",
        );
        ext::refresh_extensions(output);
    } else {
        output.error("HITL Mount", "Some extensions failed to mount");
        std::process::exit(1);
    }
}

/// Create extension directory with proper error handling
fn create_extension_directory(
    dir_path: &str,
    output: &OutputManager,
) -> Result<(), std::io::Error> {
    if !Path::new(dir_path).exists() {
        fs::create_dir_all(dir_path)?;
        output.progress(&format!("Created directory: {dir_path}"));
    } else {
        output.progress(&format!("Directory already exists: {dir_path}"));
    }
    Ok(())
}

/// Mount NFS extension with proper error handling
fn mount_nfs_extension(
    server_ip: &str,
    server_port: &str,
    extension: &str,
    mount_point: &str,
    output: &OutputManager,
) -> Result<(), HitlError> {
    let nfs_source = format!("{server_ip}:/{extension}");
    let mount_options = format!("port={server_port},vers=4,hard,timeo=600,retrans=2,acregmin=0,acregmax=1,acdirmin=0,acdirmax=1,lookupcache=none");

    output.step(
        "NFS Mount",
        &format!("Mounting {nfs_source} to {mount_point}"),
    );

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
        .map_err(|e| HitlError::Command {
            command: command_name.to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HitlError::Mount {
            extension: extension.to_string(),
            mount_point: mount_point.to_string(),
            error: stderr.to_string(),
        });
    }

    Ok(())
}

/// Unmount NFS extensions
fn unmount_extensions(matches: &ArgMatches, output: &OutputManager) {
    let extensions: Vec<&String> = matches
        .get_many::<String>("extension")
        .expect("at least one extension is required")
        .collect();

    output.info(
        "HITL Unmount",
        &format!("Unmounting {} extension(s)", extensions.len()),
    );

    // Step 1: Unmerge extensions first
    output.step("HITL Unmount", "Unmerging extensions");
    ext::unmerge_extensions(false, output);

    let extensions_base_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/avocado/hitl")
    } else {
        "/run/avocado/hitl".to_string()
    };

    let mut success = true;

    // Step 2: Unmount NFS shares and clean up directories
    for extension in &extensions {
        output.step(
            "HITL Unmount",
            &format!("Unmounting extension: {extension}"),
        );

        let extension_dir = format!("{extensions_base_dir}/{extension}");

        // Unmount NFS share
        if let Err(e) = unmount_nfs_extension(&extension_dir, output) {
            output.error(
                "HITL Unmount",
                &format!("Failed to unmount extension {extension}: {e}"),
            );
            success = false;
            continue;
        }

        // Remove the directory
        if let Err(e) = cleanup_extension_directory(&extension_dir, output) {
            output.error(
                "HITL Unmount",
                &format!("Failed to cleanup directory for {extension}: {e}"),
            );
            success = false;
            continue;
        }

        output.progress(&format!("Successfully unmounted extension: {extension}"));
    }

    if success {
        output.success("HITL Unmount", "All extensions unmounted successfully");
        output.info("HITL Unmount", "Refreshing extensions to apply changes");
        // Step 3: Merge remaining extensions
        ext::merge_extensions(output);
    } else {
        output.error("HITL Unmount", "Some extensions failed to unmount");
        std::process::exit(1);
    }
}

/// Unmount NFS extension with proper error handling
fn unmount_nfs_extension(mount_point: &str, output: &OutputManager) -> Result<(), HitlError> {
    // Check if the directory is actually mounted
    if !Path::new(mount_point).exists() {
        output.progress(&format!("Directory doesn't exist: {mount_point}"));
        return Ok(());
    }

    output.step("NFS Unmount", &format!("Unmounting {mount_point}"));

    // Check if we're in test mode and should use mock commands
    let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        "mock-umount"
    } else {
        "umount"
    };

    let output = ProcessCommand::new(command_name)
        .args([mount_point])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| HitlError::Command {
            command: command_name.to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HitlError::Unmount {
            mount_point: mount_point.to_string(),
            error: stderr.to_string(),
        });
    }

    Ok(())
}

/// Clean up extension directory after unmounting
fn cleanup_extension_directory(
    dir_path: &str,
    output: &OutputManager,
) -> Result<(), std::io::Error> {
    if Path::new(dir_path).exists() {
        fs::remove_dir_all(dir_path)?;
        output.progress(&format!("Removed directory: {dir_path}"));
    } else {
        output.progress(&format!("Directory already removed: {dir_path}"));
    }
    Ok(())
}

/// Errors related to HITL operations
#[derive(Debug, thiserror::Error)]
pub enum HitlError {
    #[error("Failed to run command '{command}': {source}")]
    Command {
        command: String,
        source: std::io::Error,
    },

    #[error("Failed to mount extension '{extension}' to '{mount_point}': {error}")]
    Mount {
        extension: String,
        mount_point: String,
        error: String,
    },

    #[error("Failed to unmount '{mount_point}': {error}")]
    Unmount { mount_point: String, error: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_command() {
        let cmd = create_command();
        assert_eq!(cmd.get_name(), "hitl");

        // Check that both mount and unmount subcommands exist
        let subcommands: Vec<_> = cmd.get_subcommands().collect();
        assert_eq!(subcommands.len(), 2);

        let subcommand_names: Vec<&str> = subcommands.iter().map(|cmd| cmd.get_name()).collect();
        assert!(subcommand_names.contains(&"mount"));
        assert!(subcommand_names.contains(&"unmount"));
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

    #[test]
    fn test_unmount_command_args() {
        let cmd = create_command();
        let unmount_cmd = cmd
            .get_subcommands()
            .find(|subcmd| subcmd.get_name() == "unmount")
            .expect("unmount subcommand should exist");

        // Check required arguments
        let args: Vec<_> = unmount_cmd.get_arguments().collect();
        let arg_names: Vec<&str> = args.iter().map(|arg| arg.get_id().as_str()).collect();

        assert!(arg_names.contains(&"extension"));
    }
}
