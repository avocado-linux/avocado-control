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

/// Helper function to run avocadoctl with environment variables
fn run_avocadoctl_with_env(args: &[&str], env_vars: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(get_binary_path());
    cmd.args(args);

    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    cmd.output().expect("Failed to execute avocadoctl")
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
        stdout.contains("status"),
        "Help should mention status subcommand"
    );
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

/// Test status command
#[test]
fn test_status_command() {
    let output = run_avocadoctl(&["status", "--help"]);
    assert!(output.status.success(), "Status help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Show overall system status including extensions"),
        "Should show status description"
    );
}

/// Test status command with mocks
#[test]
fn test_status_with_mocks() {
    // Setup mock environment
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Add fixtures path to PATH so mock binaries can be found
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &["status"],
        &[("AVOCADO_TEST_MODE", "1"), ("PATH", &new_path)],
    );

    assert!(output.status.success(), "Status should succeed with mocks");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Avocado Extension Status"),
        "Should show extension status header"
    );
    assert!(stdout.contains("Summary:"), "Should show status summary");
    assert!(
        stdout.contains("config-ext-1"),
        "Should show configuration extension in output"
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

/// Test top-level command aliases
#[test]
fn test_top_level_aliases() {
    // Test help shows the aliases
    let help_output = run_avocadoctl(&["--help"]);
    assert!(help_output.status.success(), "Help should succeed");
    
    let stdout = String::from_utf8_lossy(&help_output.stdout);
    assert!(stdout.contains("merge"), "Should contain merge alias");
    assert!(stdout.contains("unmerge"), "Should contain unmerge alias");
    assert!(stdout.contains("refresh"), "Should contain refresh alias");
    assert!(stdout.contains("alias for 'ext merge'"), "Should indicate merge is an alias");
    
    // Test merge help works
    let merge_help = run_avocadoctl(&["merge", "--help"]);
    assert!(merge_help.status.success(), "Merge help should succeed");
    
    // Test unmerge help works
    let unmerge_help = run_avocadoctl(&["unmerge", "--help"]);
    assert!(unmerge_help.status.success(), "Unmerge help should succeed");
    
    // Test refresh help works
    let refresh_help = run_avocadoctl(&["refresh", "--help"]);
    assert!(refresh_help.status.success(), "Refresh help should succeed");
}
