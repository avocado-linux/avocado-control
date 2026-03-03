use crate::commands::ext;
use crate::commands::hitl;
use crate::config::Config;
use crate::output::OutputManager;
use crate::service::error::AvocadoError;
use std::fs;
use std::path::Path;
use std::process::{Command as ProcessCommand, Stdio};

/// A quiet OutputManager for service-layer calls.
fn quiet_output() -> OutputManager {
    OutputManager::new(false, false)
}

/// Mount NFS extensions from a remote server.
pub fn mount(
    server_ip: &str,
    server_port: Option<&str>,
    extensions: &[String],
) -> Result<(), AvocadoError> {
    let output = quiet_output();
    let port = server_port.unwrap_or("12049");

    let extensions_base_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("AVOCADO_TEST_TMPDIR")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/avocado/hitl")
    } else {
        "/run/avocado/hitl".to_string()
    };

    for extension in extensions {
        let extension_dir = format!("{extensions_base_dir}/{extension}");

        // Create directory
        if !Path::new(&extension_dir).exists() {
            fs::create_dir_all(&extension_dir)?;
        }

        // Mount NFS share
        let nfs_source = format!("{server_ip}:/{extension}");
        let mount_options = format!("port={port},vers=4,hard,timeo=600,retrans=2,acregmin=0,acregmax=1,acdirmin=0,acdirmax=1,lookupcache=none");

        let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
            "mock-systemd-mount"
        } else {
            "systemd-mount"
        };

        let result = ProcessCommand::new(command_name)
            .args([
                "--no-block",
                "--collect",
                "-t",
                "nfs4",
                "-o",
                &mount_options,
                &nfs_source,
                &extension_dir,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| AvocadoError::MountFailed {
                extension: extension.clone(),
                reason: format!("Failed to run {command_name}: {e}"),
            })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            // Clean up directory on failure
            let _ = fs::remove_dir(&extension_dir);
            return Err(AvocadoError::MountFailed {
                extension: extension.clone(),
                reason: stderr.to_string(),
            });
        }

        // Create service drop-ins for enabled services
        let enabled_services =
            ext::scan_extension_for_enable_services(Path::new(&extension_dir), extension);
        if !enabled_services.is_empty() {
            let _ = hitl::create_service_dropins(extension, &extension_dir, &enabled_services, &output);
        }
    }

    // Reload systemd
    let _ = hitl::systemd_daemon_reload(&output);

    // Refresh extensions
    let config = Config::default();
    let _ = crate::service::ext::refresh_extensions(&config);

    Ok(())
}

/// Unmount NFS extensions.
pub fn unmount(extensions: &[String]) -> Result<(), AvocadoError> {
    let output = quiet_output();

    let extensions_base_dir = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        let temp_base = std::env::var("AVOCADO_TEST_TMPDIR")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/tmp".to_string());
        format!("{temp_base}/avocado/hitl")
    } else {
        "/run/avocado/hitl".to_string()
    };

    // Step 1: Scan for enabled services before unmounting (while mounts are accessible)
    let mut extension_services: Vec<(String, Vec<String>)> = Vec::new();
    for extension in extensions {
        let extension_dir = format!("{extensions_base_dir}/{extension}");
        let enabled_services =
            ext::scan_extension_for_enable_services(Path::new(&extension_dir), extension);
        if !enabled_services.is_empty() {
            extension_services.push((extension.clone(), enabled_services));
        }
    }

    // Step 2: Clean up service drop-ins
    for (extension, services) in &extension_services {
        let _ = hitl::cleanup_service_dropins(extension, services, &output);
    }

    // Step 3: Unmount each extension
    for extension in extensions {
        let mount_point = format!("{extensions_base_dir}/{extension}");

        // Unmount
        if Path::new(&mount_point).exists() {
            let command_name = if std::env::var("AVOCADO_TEST_MODE").is_ok() {
                "mock-umount"
            } else {
                "umount"
            };

            let result = ProcessCommand::new(command_name)
                .arg(&mount_point)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .map_err(|e| AvocadoError::UnmountFailed {
                    extension: extension.clone(),
                    reason: format!("Failed to run umount: {e}"),
                })?;

            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                return Err(AvocadoError::UnmountFailed {
                    extension: extension.clone(),
                    reason: stderr.to_string(),
                });
            }

            // Clean up directory
            let _ = fs::remove_dir(&mount_point);
        }
    }

    // Reload systemd
    let _ = hitl::systemd_daemon_reload(&output);

    // Refresh extensions
    let config = Config::default();
    let _ = crate::service::ext::refresh_extensions(&config);

    Ok(())
}
