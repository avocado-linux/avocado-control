use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Helper function to get the path to the built binary
fn get_binary_path() -> PathBuf {
    let mut path = std::env::current_dir().expect("Failed to get current directory");
    path.push("target");
    path.push("debug");
    path.push("avocadoctl");
    path
}

/// Helper function to run avocadoctl with arguments and return output
fn run_avocadoctl(args: &[&str]) -> std::process::Output {
    Command::new(get_binary_path())
        .args(args)
        .output()
        .expect("Failed to execute avocadoctl")
}

/// Test that the binary exists and can be executed
#[test]
fn test_binary_exists() {
    let binary_path = get_binary_path();
    assert!(
        binary_path.exists(),
        "Binary should exist at {:?}",
        binary_path
    );
}

/// Test basic version command
#[test]
fn test_version_command() {
    let output = run_avocadoctl(&["--version"]);
    assert!(output.status.success(), "Version command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_NAME")),
        "Version output should contain package name"
    );
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "Version output should contain version number"
    );
}

/// Test help command
#[test]
fn test_help_command() {
    let output = run_avocadoctl(&["--help"]);
    assert!(output.status.success(), "Help command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_DESCRIPTION")),
        "Help should contain description"
    );
    assert!(stdout.contains("ext"), "Help should mention ext subcommand");
    assert!(
        stdout.contains("-c, --config"),
        "Help should mention config flag"
    );
    assert!(
        stdout.contains("Sets a custom config file"),
        "Help should describe config flag"
    );
}

/// Test that default behavior shows helpful message
#[test]
fn test_default_behavior() {
    let output = run_avocadoctl(&[]);
    assert!(output.status.success(), "Default command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_NAME")),
        "Should show tool name"
    );
    assert!(
        stdout.contains(env!("CARGO_PKG_DESCRIPTION")),
        "Should show tool description"
    );
    assert!(stdout.contains("--help"), "Should mention help option");
}

/// Test invalid command
#[test]
fn test_invalid_command() {
    let output = run_avocadoctl(&["invalid-command"]);
    assert!(!output.status.success(), "Invalid command should fail");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error") || stderr.contains("unrecognized"),
        "Should show error for invalid command"
    );
}

/// Test that cleanup works - this test ensures temp files don't persist
#[test]
fn test_cleanup_functionality() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let test_file = temp_dir.path().join("test_file.tmp");

    // Create a test file
    fs::write(&test_file, "test content").expect("Failed to write test file");
    assert!(test_file.exists(), "Test file should exist");

    // TempDir should clean up automatically when dropped
    let temp_path = temp_dir.path().to_path_buf();
    drop(temp_dir);

    // Give the OS a moment to clean up
    std::thread::sleep(std::time::Duration::from_millis(10));

    // The directory should no longer exist
    assert!(!temp_path.exists(), "Temp directory should be cleaned up");
}
