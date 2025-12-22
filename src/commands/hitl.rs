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
        // Use AVOCADO_TEST_TMPDIR if set (to avoid affecting TempDir::new()),
        // otherwise fall back to TMPDIR, then /tmp
        let temp_base = std::env::var("AVOCADO_TEST_TMPDIR")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/tmp".to_string());
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

        // Scan for enabled services and create drop-ins
        let enabled_services =
            ext::scan_extension_for_enable_services(Path::new(&extension_dir), extension);
        if !enabled_services.is_empty() {
            output.info(
                "HITL Mount",
                &format!(
                    "Found {} enabled service(s) in extension {}: {}",
                    enabled_services.len(),
                    extension,
                    enabled_services.join(", ")
                ),
            );
            if let Err(e) =
                create_service_dropins(extension, &extension_dir, &enabled_services, output)
            {
                output.error(
                    "HITL Mount",
                    &format!("Failed to create service drop-ins for {extension}: {e}"),
                );
                // Continue even if drop-in creation fails - the mount still succeeded
            }
        }

        output.progress(&format!("Successfully mounted extension: {extension}"));
    }

    if success {
        // Reload systemd to apply any drop-in changes
        if let Err(e) = systemd_daemon_reload(output) {
            output.error(
                "HITL Mount",
                &format!("Failed to reload systemd daemon: {e}"),
            );
            // Continue even if daemon-reload fails
        }

        output.success("HITL Mount", "All extensions mounted successfully");
        output.info(
            "HITL Mount",
            "Refreshing extensions to apply mounted changes",
        );
        let config = crate::config::Config::default();
        ext::refresh_extensions(&config, output);
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

/// Mount NFS extension using systemd-mount for proper dependency tracking
/// This ensures the mount is properly tracked by systemd and will be unmounted
/// in the correct order during shutdown (before network teardown)
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
        &format!("Mounting {nfs_source} to {mount_point} via systemd-mount"),
    );

    // Check if we're in test mode and should use mock commands
    let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        "mock-systemd-mount"
    } else {
        "systemd-mount"
    };

    // systemd-mount creates a transient mount unit that systemd tracks
    // This ensures proper shutdown ordering (unmount before network goes down)
    // --no-block allows the command to return immediately
    // --collect removes the unit after unmounting
    let result = ProcessCommand::new(command_name)
        .args([
            "--no-block",
            "--collect",
            "-t",
            "nfs4",
            "-o",
            &mount_options,
            &nfs_source,
            mount_point,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| HitlError::Command {
            command: command_name.to_string(),
            source: e,
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
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

    let extensions_base_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        // Use AVOCADO_TEST_TMPDIR if set (to avoid affecting TempDir::new()),
        // otherwise fall back to TMPDIR, then /tmp
        let temp_base = std::env::var("AVOCADO_TEST_TMPDIR")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/avocado/hitl")
    } else {
        "/run/avocado/hitl".to_string()
    };

    // Step 1: Scan for enabled services before unmerging (while mounts are still accessible)
    let mut extension_services: Vec<(String, Vec<String>)> = Vec::new();
    for extension in &extensions {
        let extension_dir = format!("{extensions_base_dir}/{extension}");
        let enabled_services =
            ext::scan_extension_for_enable_services(Path::new(&extension_dir), extension);
        if !enabled_services.is_empty() {
            output.info(
                "HITL Unmount",
                &format!(
                    "Found {} enabled service(s) in extension {}: {}",
                    enabled_services.len(),
                    extension,
                    enabled_services.join(", ")
                ),
            );
            extension_services.push((extension.to_string(), enabled_services));
        }
    }

    // Step 2: Unmerge extensions first
    output.step("HITL Unmount", "Unmerging extensions");
    ext::unmerge_extensions(false, output);

    // Step 3: Clean up service drop-ins
    for (extension, services) in &extension_services {
        if let Err(e) = cleanup_service_dropins(extension, services, output) {
            output.error(
                "HITL Unmount",
                &format!("Failed to cleanup service drop-ins for {extension}: {e}"),
            );
            // Continue even if drop-in cleanup fails
        }
    }

    // Step 4: Reload systemd to apply drop-in removals
    if !extension_services.is_empty() {
        if let Err(e) = systemd_daemon_reload(output) {
            output.error(
                "HITL Unmount",
                &format!("Failed to reload systemd daemon: {e}"),
            );
            // Continue even if daemon-reload fails
        }
    }

    let mut success = true;

    // Step 5: Unmount NFS shares and clean up directories
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
        // Step 6: Merge remaining extensions
        let config = crate::config::Config::default();
        ext::merge_extensions(&config, output);
    } else {
        output.error("HITL Unmount", "Some extensions failed to unmount");
        std::process::exit(1);
    }
}

/// Unmount NFS extension using systemd-umount for proper cleanup
/// This properly stops the transient mount unit created by systemd-mount
fn unmount_nfs_extension(mount_point: &str, output: &OutputManager) -> Result<(), HitlError> {
    // Check if the directory is actually mounted
    if !Path::new(mount_point).exists() {
        output.progress(&format!("Directory doesn't exist: {mount_point}"));
        return Ok(());
    }

    output.step(
        "NFS Unmount",
        &format!("Unmounting {mount_point} via systemd-umount"),
    );

    // Check if we're in test mode and should use mock commands
    let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        "mock-systemd-umount"
    } else {
        "systemd-umount"
    };

    // systemd-umount stops the mount unit, which properly handles NFS unmounting
    let result = ProcessCommand::new(command_name)
        .arg(mount_point)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| HitlError::Command {
            command: command_name.to_string(),
            source: e,
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
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

/// Convert a mount path to a systemd mount unit name
/// e.g., /run/avocado/hitl/my-ext -> run-avocado-hitl-my\x2dext.mount
fn systemd_escape_mount_path(path: &str) -> String {
    // Remove leading slash and replace / with -
    let without_leading_slash = path.trim_start_matches('/');
    // Escape dashes in path components (except separators)
    // Systemd mount unit names simply replace / with -
    // No escaping of dashes within path components is needed
    let escaped = without_leading_slash.replace('/', "-");
    format!("{escaped}.mount")
}

/// Create systemd drop-in files for services that depend on the HITL mount
/// This ensures services are stopped before the NFS mount is unmounted during shutdown
pub fn create_service_dropins(
    extension: &str,
    mount_point: &str,
    services: &[String],
    output: &OutputManager,
) -> Result<(), HitlError> {
    if services.is_empty() {
        return Ok(());
    }

    let mount_unit = systemd_escape_mount_path(mount_point);
    output.step(
        "Service Dependencies",
        &format!(
            "Creating drop-ins for {} service(s) to depend on {}",
            services.len(),
            mount_unit
        ),
    );

    // Determine the base directory for drop-ins
    let systemd_run_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        // Use AVOCADO_TEST_TMPDIR if set (to avoid affecting TempDir::new()),
        // otherwise fall back to TMPDIR, then /tmp
        let temp_base = std::env::var("AVOCADO_TEST_TMPDIR")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/run/systemd/system")
    } else {
        "/run/systemd/system".to_string()
    };

    // Collect service unit names for the mount unit drop-in
    let service_units: Vec<String> = services
        .iter()
        .map(|s| {
            if s.ends_with(".service") {
                s.clone()
            } else {
                format!("{s}.service")
            }
        })
        .collect();

    // Create drop-ins for each service
    for service_unit in &service_units {
        let dropin_dir = format!("{systemd_run_dir}/{service_unit}.d");
        let dropin_file = format!("{dropin_dir}/10-hitl-{extension}.conf");

        // Create the drop-in directory
        if let Err(e) = fs::create_dir_all(&dropin_dir) {
            output.error(
                "Service Dependencies",
                &format!("Failed to create drop-in directory {dropin_dir}: {e}"),
            );
            continue;
        }

        // Create the drop-in content
        // - RequiresMountsFor: Ensures the mount path is available
        // - BindsTo: Binds service lifecycle to mount (stops service when mount stops)
        // - After: Service starts after mount is ready; during shutdown, service stops BEFORE mount
        // - After=remote-fs.target: During shutdown, service stops BEFORE remote-fs.target
        //   This ensures the service is stopped before NFS mounts are unmounted
        let dropin_content = format!(
            "# Auto-generated by avocadoctl hitl mount for extension: {extension}\n\
            [Unit]\n\
            RequiresMountsFor={mount_point}\n\
            BindsTo={mount_unit}\n\
            After={mount_unit}\n\
            After=remote-fs.target\n"
        );

        // Write the drop-in file
        if let Err(e) = fs::write(&dropin_file, &dropin_content) {
            output.error(
                "Service Dependencies",
                &format!("Failed to write drop-in file {dropin_file}: {e}"),
            );
            continue;
        }

        output.progress(&format!("Created drop-in: {dropin_file}"));
    }

    // Create a drop-in for the mount unit to ensure services stop before unmount
    // This is critical for proper shutdown ordering - the mount unit needs to know
    // it should wait for services to stop before unmounting
    let mount_dropin_dir = format!("{systemd_run_dir}/{mount_unit}.d");
    let mount_dropin_file = format!("{mount_dropin_dir}/10-hitl-{extension}-services.conf");

    if let Err(e) = fs::create_dir_all(&mount_dropin_dir) {
        output.error(
            "Service Dependencies",
            &format!("Failed to create mount drop-in directory {mount_dropin_dir}: {e}"),
        );
    } else {
        // Before= ensures the mount unit stops AFTER the services stop
        // (i.e., services stop first, then mount is unmounted)
        let services_list = service_units.join(" ");
        let mount_dropin_content = format!(
            "# Auto-generated by avocadoctl hitl mount for extension: {extension}\n\
            # Ensures services are stopped before this mount is unmounted during shutdown\n\
            [Unit]\n\
            Before={services_list}\n"
        );

        if let Err(e) = fs::write(&mount_dropin_file, &mount_dropin_content) {
            output.error(
                "Service Dependencies",
                &format!("Failed to write mount drop-in file {mount_dropin_file}: {e}"),
            );
        } else {
            output.progress(&format!("Created drop-in: {mount_dropin_file}"));
        }
    }

    Ok(())
}

/// Clean up systemd drop-in files for services when unmounting HITL extensions
pub fn cleanup_service_dropins(
    extension: &str,
    services: &[String],
    output: &OutputManager,
) -> Result<(), HitlError> {
    if services.is_empty() {
        return Ok(());
    }

    output.step(
        "Service Dependencies",
        &format!(
            "Removing drop-ins for {} service(s) from extension {}",
            services.len(),
            extension
        ),
    );

    // Determine the base directory for drop-ins
    let systemd_run_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        // Use AVOCADO_TEST_TMPDIR if set (to avoid affecting TempDir::new()),
        // otherwise fall back to TMPDIR, then /tmp
        let temp_base = std::env::var("AVOCADO_TEST_TMPDIR")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/run/systemd/system")
    } else {
        "/run/systemd/system".to_string()
    };

    for service in services {
        // Ensure service name ends with .service
        let service_unit = if service.ends_with(".service") {
            service.clone()
        } else {
            format!("{service}.service")
        };

        let dropin_dir = format!("{systemd_run_dir}/{service_unit}.d");
        let dropin_file = format!("{dropin_dir}/10-hitl-{extension}.conf");

        // Remove the drop-in file if it exists
        if Path::new(&dropin_file).exists() {
            if let Err(e) = fs::remove_file(&dropin_file) {
                output.error(
                    "Service Dependencies",
                    &format!("Failed to remove drop-in file {dropin_file}: {e}"),
                );
                continue;
            }
            output.progress(&format!("Removed drop-in: {dropin_file}"));

            // Try to remove the drop-in directory if it's empty
            if let Ok(entries) = fs::read_dir(&dropin_dir) {
                if entries.count() == 0 {
                    let _ = fs::remove_dir(&dropin_dir);
                }
            }
        }
    }

    // Clean up mount unit drop-ins
    // We need to find and remove all mount unit drop-ins for this extension
    // Look for directories matching *.mount.d and files matching 10-hitl-{extension}-services.conf
    if let Ok(entries) = fs::read_dir(&systemd_run_dir) {
        for entry in entries.flatten() {
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();
            if filename_str.ends_with(".mount.d") {
                let mount_dropin_file =
                    format!("{systemd_run_dir}/{filename_str}/10-hitl-{extension}-services.conf");
                if Path::new(&mount_dropin_file).exists() {
                    if let Err(e) = fs::remove_file(&mount_dropin_file) {
                        output.error(
                            "Service Dependencies",
                            &format!(
                                "Failed to remove mount drop-in file {mount_dropin_file}: {e}"
                            ),
                        );
                    } else {
                        output.progress(&format!("Removed drop-in: {mount_dropin_file}"));

                        // Try to remove the drop-in directory if it's empty
                        let mount_dropin_dir = format!("{systemd_run_dir}/{filename_str}");
                        if let Ok(dir_entries) = fs::read_dir(&mount_dropin_dir) {
                            if dir_entries.count() == 0 {
                                let _ = fs::remove_dir(&mount_dropin_dir);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Call systemctl daemon-reload to apply drop-in changes
pub fn systemd_daemon_reload(output: &OutputManager) -> Result<(), HitlError> {
    // Skip daemon-reload in test mode
    if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        output.progress("Skipping daemon-reload in test mode");
        return Ok(());
    }

    output.step(
        "Systemd",
        "Reloading systemd daemon to apply drop-in changes",
    );

    let result = ProcessCommand::new("systemctl")
        .arg("daemon-reload")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| HitlError::Command {
            command: "systemctl daemon-reload".to_string(),
            source: e,
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        output.error("Systemd", &format!("daemon-reload failed: {stderr}"));
        return Err(HitlError::DaemonReload {
            error: stderr.to_string(),
        });
    }

    output.progress("Systemd daemon reloaded successfully");
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

    #[error("Failed to reload systemd daemon: {error}")]
    DaemonReload { error: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that modify environment variables
    static ENV_VAR_MUTEX: Mutex<()> = Mutex::new(());

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

    #[test]
    fn test_systemd_escape_mount_path() {
        // Test basic path escaping
        assert_eq!(
            systemd_escape_mount_path("/run/avocado/hitl/myext"),
            "run-avocado-hitl-myext.mount"
        );

        // Test path with dashes in component name (dashes are preserved, not escaped)
        assert_eq!(
            systemd_escape_mount_path("/run/avocado/hitl/my-extension"),
            "run-avocado-hitl-my-extension.mount"
        );

        // Test path with multiple dashes
        assert_eq!(
            systemd_escape_mount_path("/run/avocado/hitl/my-cool-ext"),
            "run-avocado-hitl-my-cool-ext.mount"
        );

        // Test path with leading slash removal
        assert_eq!(
            systemd_escape_mount_path("run/avocado/hitl/ext"),
            "run-avocado-hitl-ext.mount"
        );
    }

    #[test]
    fn test_create_and_cleanup_service_dropins() {
        use tempfile::TempDir;

        // Lock the mutex to prevent env var interference from other tests
        let _guard = ENV_VAR_MUTEX.lock().unwrap();

        // Save original environment variable values for restoration
        let original_test_mode = std::env::var("AVOCADO_TEST_MODE").ok();
        let original_test_tmpdir = std::env::var("AVOCADO_TEST_TMPDIR").ok();

        // Set up test environment - create TempDir BEFORE modifying env vars
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_string_lossy().to_string();

        std::env::set_var("AVOCADO_TEST_MODE", "1");
        // Use AVOCADO_TEST_TMPDIR to avoid affecting TempDir::new() in other tests
        std::env::set_var("AVOCADO_TEST_TMPDIR", &temp_path);

        let output = OutputManager::new(true);
        let extension = "test-ext";
        let mount_point = &format!("{temp_path}/avocado/hitl/test-ext");
        let services = vec!["nginx".to_string(), "prometheus.service".to_string()];

        // Create drop-ins
        let result = create_service_dropins(extension, mount_point, &services, &output);
        assert!(result.is_ok());

        // Verify service drop-ins were created
        let systemd_dir = format!("{temp_path}/run/systemd/system");
        let nginx_dropin = format!("{systemd_dir}/nginx.service.d/10-hitl-test-ext.conf");
        let prometheus_dropin = format!("{systemd_dir}/prometheus.service.d/10-hitl-test-ext.conf");

        assert!(Path::new(&nginx_dropin).exists());
        assert!(Path::new(&prometheus_dropin).exists());

        // Verify service drop-in content
        let nginx_content = fs::read_to_string(&nginx_dropin).unwrap();
        assert!(nginx_content.contains("[Unit]"));
        assert!(nginx_content.contains("RequiresMountsFor="));
        assert!(nginx_content.contains("BindsTo="));
        assert!(nginx_content.contains("After="));
        assert!(
            nginx_content.contains("After=remote-fs.target"),
            "Service drop-in should have After=remote-fs.target for shutdown ordering"
        );

        // Verify mount unit drop-in was created
        let mount_unit = systemd_escape_mount_path(mount_point);
        let mount_dropin = format!("{systemd_dir}/{mount_unit}.d/10-hitl-test-ext-services.conf");
        assert!(
            Path::new(&mount_dropin).exists(),
            "Mount drop-in should exist at {mount_dropin}"
        );

        // Verify mount drop-in content - should have Before= for all services
        let mount_content = fs::read_to_string(&mount_dropin).unwrap();
        assert!(mount_content.contains("[Unit]"));
        assert!(mount_content.contains("Before="));
        assert!(mount_content.contains("nginx.service"));
        assert!(mount_content.contains("prometheus.service"));

        // Clean up drop-ins
        let result = cleanup_service_dropins(extension, &services, &output);
        assert!(result.is_ok());

        // Verify service drop-ins were removed
        assert!(!Path::new(&nginx_dropin).exists());
        assert!(!Path::new(&prometheus_dropin).exists());

        // Verify mount drop-in was removed
        assert!(
            !Path::new(&mount_dropin).exists(),
            "Mount drop-in should be removed"
        );

        // Restore original environment variables
        match original_test_mode {
            Some(val) => std::env::set_var("AVOCADO_TEST_MODE", val),
            None => std::env::remove_var("AVOCADO_TEST_MODE"),
        }
        match original_test_tmpdir {
            Some(val) => std::env::set_var("AVOCADO_TEST_TMPDIR", val),
            None => std::env::remove_var("AVOCADO_TEST_TMPDIR"),
        }
    }

    #[test]
    fn test_create_service_dropins_empty_services() {
        let output = OutputManager::new(false);
        let services: Vec<String> = vec![];

        // Should return Ok without doing anything
        let result = create_service_dropins("test-ext", "/run/test", &services, &output);
        assert!(result.is_ok());
    }
}
