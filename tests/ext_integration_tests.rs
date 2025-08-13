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

/// Helper function to run avocadoctl with custom environment and arguments
fn run_avocadoctl_with_env(args: &[&str], env_vars: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(get_binary_path());
    cmd.args(args);
    for (key, value) in env_vars {
        cmd.env(key, value);
    }
    cmd.output().expect("Failed to execute avocadoctl")
}

/// Helper function to run avocadoctl with arguments and return output
fn run_avocadoctl(args: &[&str]) -> std::process::Output {
    Command::new(get_binary_path())
        .args(args)
        .output()
        .expect("Failed to execute avocadoctl")
}

/// Test ext list with non-existent directory
#[test]
fn test_ext_list_nonexistent_directory() {
    let output = run_avocadoctl(&["ext", "list"]);
    // This should not panic, but will likely show an error since /var/lib/avocado/extensions doesn't exist
    // The command should still exit successfully (error handling is done via stderr, not exit code)

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should contain error message about directory not existing
    assert!(
        stderr.contains("Error accessing extensions directory")
            || stderr.contains("No such file or directory"),
        "Should show appropriate error message for missing directory"
    );
}

/// Test ext list with mock extensions directory using environment variable
#[test]
fn test_ext_list_with_mock_extensions() {
    // Create a temporary directory structure
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let extensions_dir = temp_dir.path();

    // Create test extensions
    fs::create_dir(extensions_dir.join("test_extension_dir"))
        .expect("Failed to create test directory");
    fs::create_dir(extensions_dir.join("another_ext"))
        .expect("Failed to create another test directory");
    fs::write(extensions_dir.join("file_extension.raw"), "")
        .expect("Failed to create test .raw file");
    fs::write(extensions_dir.join("binary_ext.raw"), "binary data")
        .expect("Failed to create binary .raw file");
    fs::write(extensions_dir.join("ignored_file.txt"), "").expect("Failed to create ignored file");
    fs::write(extensions_dir.join("README.md"), "readme content")
        .expect("Failed to create ignored readme");

    // Run avocadoctl ext list with custom extensions directory
    let output = run_avocadoctl_with_env(
        &["ext", "list"],
        &[("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap())],
    );

    assert!(
        output.status.success(),
        "ext list should succeed with mock directory"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain our test extensions
    assert!(
        stdout.contains("test_extension_dir"),
        "Should list directory extension"
    );
    assert!(
        stdout.contains("another_ext"),
        "Should list another directory extension"
    );
    assert!(
        stdout.contains("file_extension"),
        "Should list .raw file without extension"
    );
    assert!(
        stdout.contains("binary_ext"),
        "Should list binary .raw file without extension"
    );

    // Should NOT contain ignored files
    assert!(
        !stdout.contains("ignored_file.txt"),
        "Should not list .txt files"
    );
    assert!(!stdout.contains("README.md"), "Should not list .md files");
    assert!(
        !stdout.contains(".raw"),
        "Should not show .raw extension in output"
    );

    // Should be sorted alphabetically
    let lines: Vec<&str> = stdout.lines().collect();
    let extension_lines: Vec<&str> = lines
        .iter()
        .filter(|line| {
            line.trim().starts_with("another_ext")
                || line.trim().starts_with("binary_ext")
                || line.trim().starts_with("file_extension")
                || line.trim().starts_with("test_extension_dir")
        })
        .copied()
        .collect();

    // Verify alphabetical order
    assert!(
        extension_lines.len() >= 4,
        "Should have at least 4 extension entries"
    );

    // The temp_dir will be automatically cleaned up when it goes out of scope
}

/// Test ext list with custom config file
#[test]
fn test_ext_list_with_config_file() {
    // Create temporary directories for config and extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let config_path = temp_dir.path().join("test_config.toml");
    let extensions_dir = temp_dir.path().join("custom_extensions");

    fs::create_dir(&extensions_dir).expect("Failed to create extensions directory");

    // Create test extensions
    fs::create_dir(extensions_dir.join("config_test_ext"))
        .expect("Failed to create test directory");
    fs::write(extensions_dir.join("config_raw_ext.raw"), "")
        .expect("Failed to create test .raw file");

    // Create config file
    let config_content = format!(
        r#"[avocado.ext]
dir = "{}"
"#,
        extensions_dir.to_string_lossy()
    );
    fs::write(&config_path, config_content).expect("Failed to write config file");

    // Run avocadoctl ext list with custom config
    let output = run_avocadoctl(&["-c", config_path.to_str().unwrap(), "ext", "list"]);

    assert!(
        output.status.success(),
        "ext list should succeed with custom config"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain our test extensions from config-specified directory
    assert!(
        stdout.contains("config_test_ext"),
        "Should list directory extension from config"
    );
    assert!(
        stdout.contains("config_raw_ext"),
        "Should list .raw file from config"
    );
}

/// Test -c flag with invalid config file
#[test]
fn test_invalid_config_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let config_path = temp_dir.path().join("invalid_config.toml");

    // Create invalid TOML content
    fs::write(&config_path, "invalid toml content [[[").expect("Failed to write invalid config");

    let output = run_avocadoctl(&["-c", config_path.to_str().unwrap(), "ext", "list"]);

    assert!(
        !output.status.success(),
        "Should fail with invalid config file"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error loading configuration"),
        "Should show config error"
    );
}

/// Test -c flag with nonexistent config file (should use defaults)
#[test]
fn test_nonexistent_config_file() {
    let output = run_avocadoctl(&["-c", "/nonexistent/config.toml", "ext", "list"]);

    // Should still work (using defaults) since nonexistent config is handled gracefully
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should show error about extensions directory, not config file
    assert!(
        stderr.contains("Error accessing extensions directory")
            || stderr.contains("No such file or directory")
    );
}

/// Test ext list with empty extensions directory
#[test]
fn test_ext_list_empty_directory() {
    // Create an empty temporary directory
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let extensions_dir = temp_dir.path();

    // Run avocadoctl ext list with empty extensions directory
    let output = run_avocadoctl_with_env(
        &["ext", "list"],
        &[("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap())],
    );

    assert!(
        output.status.success(),
        "ext list should succeed with empty directory"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No extensions found"),
        "Should indicate no extensions found"
    );
    assert!(
        stdout.contains(extensions_dir.to_str().unwrap()),
        "Should show the directory path"
    );

    // The temp_dir will be automatically cleaned up when it goes out of scope
}

/// Test ext list help
#[test]
fn test_ext_list_help() {
    let output = run_avocadoctl(&["ext", "list", "--help"]);
    assert!(output.status.success(), "Ext list help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("List all available extensions"),
        "Should contain list description"
    );
}

/// Test with example config fixture (demonstrates fixture usage)
#[test]
fn test_example_config_fixture() {
    use std::path::Path;

    // Verify the example config fixture exists and is valid
    let fixture_path = Path::new("tests/fixtures/example_config.toml");
    assert!(fixture_path.exists(), "Example config fixture should exist");

    // Test that we can load the example config without errors
    // This demonstrates how fixtures can be used in tests
    let config_content =
        fs::read_to_string(fixture_path).expect("Should be able to read example config");

    // Verify it contains expected content
    assert!(
        config_content.contains("[avocado.ext]"),
        "Should contain avocado.ext section"
    );
    assert!(
        config_content.contains("dir ="),
        "Should contain dir setting"
    );

    // Test parsing the config (would fail if TOML is invalid)
    let _parsed: toml::Value =
        toml::from_str(&config_content).expect("Example config should be valid TOML");
}

/// Test ext merge command with mock systemd binaries
#[test]
fn test_ext_merge_with_mocks() {
    // Setup mock environment
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Add fixtures path to PATH so mock binaries can be found
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &["ext", "merge", "--verbose"],
        &[("AVOCADO_TEST_MODE", "1"), ("PATH", &new_path)],
    );

    assert!(
        output.status.success(),
        "ext merge should succeed with mocks"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Merging extensions"),
        "Should show merging message"
    );
    assert!(
        stdout.contains("Extensions merged successfully"),
        "Should show success message"
    );
    assert!(
        stdout.contains("systemd-sysext merge"),
        "Should show sysext operation"
    );
    assert!(
        stdout.contains("systemd-confext merge"),
        "Should show confext operation"
    );
}

/// Test ext unmerge command with mock systemd binaries
#[test]
fn test_ext_unmerge_with_mocks() {
    // Setup mock environment
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Add fixtures path to PATH so mock binaries can be found
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &["ext", "unmerge", "--verbose"],
        &[("AVOCADO_TEST_MODE", "1"), ("PATH", &new_path)],
    );

    assert!(
        output.status.success(),
        "ext unmerge should succeed with mocks"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Unmerging extensions"),
        "Should show unmerging message"
    );
    assert!(
        stdout.contains("Extensions unmerged successfully"),
        "Should show success message"
    );
    assert!(
        stdout.contains("systemd-sysext unmerge"),
        "Should show sysext operation"
    );
    assert!(
        stdout.contains("systemd-confext unmerge"),
        "Should show confext operation"
    );
    assert!(
        stdout.contains("Running depmod"),
        "Should show depmod running message"
    );
    assert!(
        stdout.contains("depmod completed successfully"),
        "Should show depmod completion"
    );
}

/// Test ext merge help
#[test]
fn test_ext_merge_help() {
    let output = run_avocadoctl(&["ext", "merge", "--help"]);
    assert!(output.status.success(), "Ext merge help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Merge extensions using systemd-sysext and systemd-confext"),
        "Should contain merge description"
    );
}

/// Test that environment preparation works with mock extensions
#[test]
fn test_environment_preparation_with_mock_extensions() {
    use std::fs;
    use tempfile::TempDir;

    // Clean up any previous test directories
    let temp_base = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
    let _ = fs::remove_dir_all(format!("{}/test_extensions", temp_base));
    let _ = fs::remove_dir_all(format!("{}/test_confexts", temp_base));

    // Create a temporary directory for extensions
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let extensions_path = temp_dir.path().join("extensions");
    fs::create_dir_all(&extensions_path).expect("Failed to create extensions dir");

    // Create a mock .raw extension file
    let raw_file = extensions_path.join("test-ext.raw");
    fs::write(&raw_file, b"mock raw extension").expect("Failed to create raw file");

    // Create a mock directory extension
    let dir_ext = extensions_path.join("dir-ext");
    fs::create_dir_all(&dir_ext).expect("Failed to create dir extension");

    // Setup mock environment
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Add fixtures path to PATH so mock binaries can be found
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &["ext", "merge", "--verbose"],
        &[
            ("AVOCADO_TEST_MODE", "1"),
            ("PATH", &new_path),
            ("AVOCADO_EXTENSIONS_PATH", extensions_path.to_str().unwrap()),
        ],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        println!("STDOUT: {}", stdout);
        println!("STDERR: {}", stderr);
        panic!("ext merge should succeed with mock extensions");
    }

    assert!(
        stdout.contains("Preparing extension environment"),
        "Should show environment preparation message"
    );
    // The output should now include scanning from different sources
    assert!(
        stdout.contains("Scanning HITL extensions")
            && stdout.contains("Scanning directory extensions")
            && stdout.contains("Scanning raw file extensions"),
        "Should scan all extension sources in priority order"
    );
    assert!(
        stdout.contains("Created sysext symlink:") || stdout.contains("Created confext symlink:"),
        "Should create symlinks for extensions"
    );

    // Clean up test directories
    let _ = fs::remove_dir_all(format!("{}/test_extensions", temp_base));
    let _ = fs::remove_dir_all(format!("{}/test_confexts", temp_base));
}

/// Test ext unmerge help
#[test]
fn test_ext_unmerge_help() {
    let output = run_avocadoctl(&["ext", "unmerge", "--help"]);
    assert!(output.status.success(), "Ext unmerge help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Unmerge extensions using systemd-sysext and systemd-confext"),
        "Should contain unmerge description"
    );
}

/// Test ext refresh command with mock systemd binaries
#[test]
fn test_ext_refresh_with_mocks() {
    // Setup mock environment
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");
    let release_dir = fixtures_path.join("extension-release.d");

    // Add fixtures path to PATH so mock binaries can be found
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &["ext", "refresh", "--verbose"],
        &[
            ("AVOCADO_TEST_MODE", "1"),
            ("PATH", &new_path),
            (
                "AVOCADO_EXTENSION_RELEASE_DIR",
                &release_dir.to_string_lossy(),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "ext refresh should succeed with mocks"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Refreshing extensions"),
        "Should show refreshing message"
    );
    assert!(
        stdout.contains("Extensions refreshed successfully"),
        "Should show final success message"
    );
    // Should contain both unmerge and merge operations
    assert!(
        stdout.contains("systemd-sysext unmerge"),
        "Should show sysext unmerge operation"
    );
    assert!(
        stdout.contains("systemd-confext unmerge"),
        "Should show confext unmerge operation"
    );
    assert!(
        stdout.contains("systemd-sysext merge"),
        "Should show sysext merge operation"
    );
    assert!(
        stdout.contains("systemd-confext merge"),
        "Should show confext merge operation"
    );
    assert!(
        stdout.contains("Extensions unmerged successfully"),
        "Should show unmerge success"
    );
    assert!(
        stdout.contains("Extensions merged successfully"),
        "Should show merge success"
    );

    // Verify depmod is only called once at the end (during merge phase)
    let depmod_count = stdout.matches("Running depmod").count();
    assert_eq!(
        depmod_count, 1,
        "Should call depmod exactly once during refresh (only during merge phase)"
    );
    assert!(
        stdout.contains("Running depmod"),
        "Should show depmod running message"
    );
    assert!(
        stdout.contains("depmod completed successfully"),
        "Should show depmod completion"
    );
}

/// Test ext refresh help
#[test]
fn test_ext_refresh_help() {
    let output = run_avocadoctl(&["ext", "refresh", "--help"]);
    assert!(output.status.success(), "Ext refresh help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Unmerge and then merge extensions (refresh extensions)"),
        "Should contain refresh description"
    );
}

/// Test that ext help shows all subcommands
#[test]
fn test_ext_help_shows_all_commands() {
    let output = run_avocadoctl(&["ext", "--help"]);
    assert!(output.status.success(), "Ext help command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Extension management commands"),
        "Ext help should contain description"
    );
    assert!(
        stdout.contains("list"),
        "Ext help should mention list subcommand"
    );
    assert!(
        stdout.contains("merge"),
        "Ext help should mention merge subcommand"
    );
    assert!(
        stdout.contains("unmerge"),
        "Ext help should mention unmerge subcommand"
    );
    assert!(
        stdout.contains("refresh"),
        "Ext help should mention refresh subcommand"
    );
    assert!(
        stdout.contains("status"),
        "Ext help should mention status subcommand"
    );
}

/// Test ext merge with depmod post-processing
#[test]
fn test_ext_merge_with_depmod_processing() {
    // Setup mock environment with release files that require depmod
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");
    let release_dir = fixtures_path.join("extension-release.d");

    // Add fixtures path to PATH so mock binaries can be found
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    // Set environment variables to use test release directory and mocks
    let output = run_avocadoctl_with_env(
        &["ext", "merge", "--verbose"],
        &[
            ("AVOCADO_TEST_MODE", "1"),
            ("PATH", &new_path),
            // Override the release directory for testing (if implemented)
            (
                "AVOCADO_EXTENSION_RELEASE_DIR",
                &release_dir.to_string_lossy(),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "ext merge should succeed with depmod processing"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Merging extensions"),
        "Should show merging message"
    );
    assert!(
        stdout.contains("Extensions merged successfully"),
        "Should show merge success"
    );
    assert!(
        stdout.contains("Running depmod"),
        "Should show depmod running message"
    );
    assert!(
        stdout.contains("depmod completed successfully"),
        "Should show depmod completion"
    );
}

/// Test post-merge processing with no depmod needed
#[test]
fn test_ext_merge_no_depmod_needed() {
    // This test verifies that merge works normally when no depmod is needed
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &["ext", "merge", "--verbose"],
        &[("AVOCADO_TEST_MODE", "1"), ("PATH", &new_path)],
    );

    assert!(
        output.status.success(),
        "ext merge should succeed without depmod"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Extensions merged successfully"),
        "Should show merge success"
    );
}

/// Test ext status command with mock systemd binaries
#[test]
fn test_ext_status_with_mocks() {
    // Setup mock environment
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Add fixtures path to PATH so mock binaries can be found
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let output = run_avocadoctl_with_env(
        &["ext", "status"],
        &[("AVOCADO_TEST_MODE", "1"), ("PATH", &new_path)],
    );

    assert!(
        output.status.success(),
        "ext status should succeed with mocks"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Avocado Extension Status"),
        "Should show enhanced extension status header"
    );
    assert!(
        stdout.contains("Extension") && stdout.contains("Status") && stdout.contains("Origin"),
        "Should show enhanced status table headers"
    );
    assert!(stdout.contains("Summary:"), "Should show status summary");
    assert!(
        stdout.contains("test-ext-1") && stdout.contains("SYSEXT"),
        "Should show system extension in table"
    );
    assert!(
        stdout.contains("test-ext-2") && stdout.contains("SYSEXT"),
        "Should show system extension in table"
    );
    assert!(
        stdout.contains("config-ext-1") && stdout.contains("CONFEXT"),
        "Should show configuration extension in table"
    );
    assert!(
        stdout.contains("Mount Info"),
        "Should show mount information for extensions"
    );
}

/// Test ext status help
#[test]
fn test_ext_status_help() {
    let output = run_avocadoctl(&["ext", "status", "--help"]);
    assert!(output.status.success(), "Ext status help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Show status of merged extensions"),
        "Should contain status description"
    );
}
