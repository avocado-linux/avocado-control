use std::process::Command;
use tempfile::TempDir;

/// Helper function to run avocadoctl with environment variables
fn run_avocadoctl_with_env(args: &[&str], env_vars: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new("./target/debug/avocadoctl");
    for (key, value) in env_vars {
        cmd.env(key, value);
    }
    cmd.args(args)
        .output()
        .expect("Failed to execute avocadoctl")
}

/// Helper function to run avocadoctl
fn run_avocadoctl(args: &[&str]) -> std::process::Output {
    Command::new("./target/debug/avocadoctl")
        .args(args)
        .output()
        .expect("Failed to execute avocadoctl")
}

/// Test hitl help command
#[test]
fn test_hitl_help() {
    let output = run_avocadoctl(&["hitl", "--help"]);
    assert!(output.status.success(), "Hitl help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Hardware-in-the-loop (HITL) testing commands"),
        "Should contain HITL description"
    );
    assert!(stdout.contains("mount"), "Should mention mount subcommand");
}

/// Test hitl mount help command
#[test]
fn test_hitl_mount_help() {
    let output = run_avocadoctl(&["hitl", "mount", "--help"]);
    assert!(output.status.success(), "Hitl mount help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Mount NFS extensions from a remote server"),
        "Should contain mount description"
    );
    assert!(
        stdout.contains("--server-ip"),
        "Should mention server-ip option"
    );
    assert!(
        stdout.contains("--server-port"),
        "Should mention server-port option"
    );
    assert!(
        stdout.contains("--extension"),
        "Should mention extension option"
    );
    assert!(
        stdout.contains("-s, --server-ip"),
        "Should show short option for server-ip"
    );
    assert!(
        stdout.contains("-p, --server-port"),
        "Should show short option for server-port"
    );
    assert!(
        stdout.contains("-e, --extension"),
        "Should show short option for extension"
    );
}

/// Test hitl mount command with mock
#[test]
fn test_hitl_mount_with_mocks() {
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Create a temporary directory to simulate /var/lib/avocado/extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_extensions_dir = temp_dir.path();

    // Add fixtures path to PATH so mock binaries can be found
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &[
            "hitl",
            "mount",
            "--server-ip",
            "192.168.1.10",
            "--server-port",
            "12049",
            "--extension",
            "foo",
            "--extension",
            "avocado-dev",
            "--verbose",
        ],
        &[
            ("AVOCADO_TEST_MODE", "1"),
            ("PATH", &new_path),
            (
                "AVOCADO_EXTENSIONS_PATH",
                &temp_extensions_dir.to_string_lossy(),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "Hitl mount should succeed with mocks: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Mounting extensions from 192.168.1.10:12049"),
        "Should show mounting message"
    );
    assert!(
        stdout.contains("Setting up extension: foo"),
        "Should show setup for foo extension"
    );
    assert!(
        stdout.contains("Setting up extension: avocado-dev"),
        "Should show setup for avocado-dev extension"
    );
    assert!(
        stdout.contains("All extensions mounted successfully"),
        "Should show success message"
    );
    assert!(
        stdout.contains("Refreshing extensions to apply mounted changes"),
        "Should show extension refresh message"
    );
    assert!(
        stdout.contains("Starting extension refresh process"),
        "Should call ext refresh"
    );
    assert!(
        stdout.contains("Extensions refreshed successfully"),
        "Should complete extension refresh"
    );
    assert!(
        stdout.contains("Scanning HITL extensions"),
        "Should scan HITL extensions during refresh"
    );
}

/// Test hitl mount with short options
#[test]
fn test_hitl_mount_short_options() {
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Create a temporary directory to simulate /var/lib/avocado/extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_extensions_dir = temp_dir.path();

    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &[
            "hitl",
            "mount",
            "-s",
            "192.168.1.20",
            "-p",
            "2049",
            "-e",
            "test-ext",
            "-v",
        ],
        &[
            ("AVOCADO_TEST_MODE", "1"),
            ("PATH", &new_path),
            (
                "AVOCADO_EXTENSIONS_PATH",
                &temp_extensions_dir.to_string_lossy(),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "Hitl mount with short options should succeed"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Mounting extensions from 192.168.1.20:2049"),
        "Should show correct server and port"
    );
    assert!(
        stdout.contains("Setting up extension: test-ext"),
        "Should show setup for test-ext extension"
    );
}

/// Test hitl mount missing required arguments
#[test]
fn test_hitl_mount_missing_args() {
    let output = run_avocadoctl(&["hitl", "mount"]);
    assert!(
        !output.status.success(),
        "Hitl mount should fail without required arguments"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("required") || stderr.contains("missing"),
        "Should show error about missing required arguments"
    );
}

/// Test hitl mount with default port
#[test]
fn test_hitl_mount_default_port() {
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Create a temporary directory to simulate /var/lib/avocado/extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_extensions_dir = temp_dir.path();

    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &[
            "hitl",
            "mount",
            "--server-ip",
            "192.168.1.30",
            "--extension",
            "default-port-test",
            "--verbose",
        ],
        &[
            ("AVOCADO_TEST_MODE", "1"),
            ("PATH", &new_path),
            (
                "AVOCADO_EXTENSIONS_PATH",
                &temp_extensions_dir.to_string_lossy(),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "Hitl mount should succeed with default port"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Mounting extensions from 192.168.1.30:12049"),
        "Should use default port 12049"
    );
}

/// Test hitl unmount help command
#[test]
fn test_hitl_unmount_help() {
    let output = run_avocadoctl(&["hitl", "unmount", "--help"]);
    assert!(output.status.success(), "Hitl unmount help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Unmount NFS extensions"),
        "Should contain unmount description"
    );
    assert!(
        stdout.contains("--extension"),
        "Should mention extension option"
    );
    assert!(
        stdout.contains("-e, --extension"),
        "Should show short option for extension"
    );
}

/// Test hitl unmount command with mock
#[test]
fn test_hitl_unmount_with_mocks() {
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Create a temporary directory to simulate /var/lib/avocado/extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_extensions_dir = temp_dir.path();

    // Add fixtures path to PATH so mock binaries can be found
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &[
            "hitl",
            "unmount",
            "--extension",
            "foo",
            "--extension",
            "avocado-dev",
            "--verbose",
        ],
        &[
            ("AVOCADO_TEST_MODE", "1"),
            ("PATH", &new_path),
            (
                "AVOCADO_EXTENSIONS_PATH",
                &temp_extensions_dir.to_string_lossy(),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "Hitl unmount should succeed with mocks: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Unmounting 2 extension(s)"),
        "Should show unmounting message"
    );
    assert!(
        stdout.contains("Unmerging extensions"),
        "Should show unmerge step"
    );
    assert!(
        stdout.contains("Unmounting extension: foo"),
        "Should show unmount for foo extension"
    );
    assert!(
        stdout.contains("Unmounting extension: avocado-dev"),
        "Should show unmount for avocado-dev extension"
    );
    assert!(
        stdout.contains("All extensions unmounted successfully"),
        "Should show success message"
    );
    assert!(
        stdout.contains("Starting extension merge process"),
        "Should show merge step at the end"
    );
}

/// Test hitl unmount with short options
#[test]
fn test_hitl_unmount_short_options() {
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Create a temporary directory
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_extensions_dir = temp_dir.path();

    // Add fixtures path to PATH
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &["hitl", "unmount", "-e", "foo", "--verbose"],
        &[
            ("AVOCADO_TEST_MODE", "1"),
            ("PATH", &new_path),
            (
                "AVOCADO_EXTENSIONS_PATH",
                &temp_extensions_dir.to_string_lossy(),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "Hitl unmount should succeed with short options"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Unmounting 1 extension(s)"),
        "Should show unmounting single extension"
    );
}

/// Test that main help shows hitl command
#[test]
fn test_main_help_shows_hitl() {
    let output = run_avocadoctl(&["--help"]);
    assert!(output.status.success(), "Main help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hitl"),
        "Main help should mention hitl command"
    );
}

/// Test that hitl help shows both mount and unmount
#[test]
fn test_hitl_help_shows_both_subcommands() {
    let output = run_avocadoctl(&["hitl", "--help"]);
    assert!(output.status.success(), "Hitl help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("mount"), "Should mention mount subcommand");
    assert!(
        stdout.contains("unmount"),
        "Should mention unmount subcommand"
    );
}

/// Test that failed HITL mount operations clean up directories
#[test]
fn test_hitl_mount_failure_cleanup() {
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Create a temporary directory
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_extensions_dir = temp_dir.path().join("avocado/hitl");

    // Create a failing mock-mount script in a temp directory
    let temp_bin_dir = temp_dir.path().join("bin");
    std::fs::create_dir_all(&temp_bin_dir).expect("Failed to create temp bin directory");

    let mock_mount_fail_path = temp_bin_dir.join("mock-mount");
    std::fs::write(&mock_mount_fail_path, r#"#!/bin/bash
# Mock mount command that fails
echo "mount.nfs4: mounting 10.0.2.2:/test-extension failed, reason given by server: No such file or directory" >&2
exit 1
"#).expect("Failed to write failing mock-mount");

    // Make it executable
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&mock_mount_fail_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&mock_mount_fail_path, perms).unwrap();

    // Add temp bin path to PATH (before fixtures so our failing mock takes precedence)
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}:{}", temp_bin_dir.to_string_lossy(), fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &["hitl", "mount", "-s", "10.0.2.2", "-e", "test-extension"],
        &[
            ("AVOCADO_TEST_MODE", "1"),
            ("PATH", &new_path),
            ("TMPDIR", &temp_dir.path().to_string_lossy()),
        ],
    );

    // The mount should fail
    assert!(
        !output.status.success(),
        "Hitl mount should fail with mock failure"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Failed to mount extension test-extension"),
        "Should show mount failure message"
    );

    // Verify the directory was cleaned up - it should not exist
    let extension_dir = temp_extensions_dir.join("test-extension");
    assert!(
        !extension_dir.exists(),
        "Extension directory should be cleaned up after mount failure"
    );
}
