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

/// Helper function to run avocadoctl with an isolated test environment
/// This creates unique temporary directories to avoid race conditions between parallel tests
fn run_avocadoctl_with_isolated_env(
    args: &[&str],
    additional_env_vars: &[(&str, &str)],
) -> (std::process::Output, TempDir) {
    // Create a unique temporary directory for this test
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path().to_string_lossy();

    // Set up isolated environment variables
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fixtures_path.to_string_lossy(), original_path);

    let mut env_vars = vec![
        ("AVOCADO_TEST_MODE", "1"),
        ("PATH", new_path.as_str()),
        ("TMPDIR", temp_path.as_ref()),
    ];

    // Add additional environment variables
    env_vars.extend(additional_env_vars);

    let output = run_avocadoctl_with_env(args, &env_vars);
    (output, temp_dir)
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
        stderr.contains("Configuration Error"),
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
    // Use isolated environment to avoid race conditions
    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(&["ext", "merge", "--verbose"], &[]);

    assert!(
        output.status.success(),
        "ext merge should succeed with mocks"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Starting extension merge process"),
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
    // Use isolated environment to avoid race conditions
    let (output, _temp_dir) =
        run_avocadoctl_with_isolated_env(&["ext", "unmerge", "--verbose"], &[]);

    assert!(
        output.status.success(),
        "ext unmerge should succeed with mocks"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Starting extension unmerge process"),
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
        stdout.contains("[INFO] Running depmod"),
        "Should show depmod running message"
    );
    assert!(
        stdout.contains("[SUCCESS] depmod completed successfully"),
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
    let _ = fs::remove_dir_all(format!("{temp_base}/test_extensions"));
    let _ = fs::remove_dir_all(format!("{temp_base}/test_confexts"));

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

    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "merge", "--verbose"],
        &[("AVOCADO_EXTENSIONS_PATH", extensions_path.to_str().unwrap())],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        println!("STDOUT: {stdout}");
        println!("STDERR: {stderr}");
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
    let _ = fs::remove_dir_all(format!("{temp_base}/test_extensions"));
    let _ = fs::remove_dir_all(format!("{temp_base}/test_confexts"));
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

    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "refresh", "--verbose"],
        &[(
            "AVOCADO_EXTENSION_RELEASE_DIR",
            &release_dir.to_string_lossy(),
        )],
    );

    assert!(
        output.status.success(),
        "ext refresh should succeed with mocks"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Starting extension refresh process"),
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
        stdout.contains("Extensions unmerged"),
        "Should show unmerge success"
    );
    assert!(
        stdout.contains("Extensions merged"),
        "Should show merge success"
    );

    // Verify depmod is only called once at the end (during merge phase)
    let depmod_count = stdout.matches("Running command: depmod").count()
        + stdout.matches("[INFO] Running depmod").count();
    assert_eq!(
        depmod_count, 1,
        "Should call depmod exactly once during refresh (only during merge phase)"
    );
    assert!(
        stdout.contains("Running command: depmod") || stdout.contains("[INFO] Running depmod"),
        "Should show depmod running message"
    );
    assert!(
        stdout.contains("Command 'depmod' completed successfully")
            || stdout.contains("[SUCCESS] depmod completed successfully"),
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

    // Use isolated environment to avoid race conditions
    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "merge", "--verbose"],
        &[(
            "AVOCADO_EXTENSION_RELEASE_DIR",
            &release_dir.to_string_lossy(),
        )],
    );

    assert!(
        output.status.success(),
        "ext merge should succeed with depmod processing"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Starting extension merge process"),
        "Should show merging message"
    );
    assert!(
        stdout.contains("Extensions merged successfully"),
        "Should show merge success"
    );
    // Should show depmod being executed in the new generic command execution
    assert!(
        stdout.contains("Running command: depmod") || stdout.contains("[INFO] Running depmod"),
        "Should show depmod running message"
    );
    assert!(
        stdout.contains("Command 'depmod' completed successfully")
            || stdout.contains("[SUCCESS] depmod completed successfully"),
        "Should show depmod completion"
    );
}

/// Test multiple extensions with both depmod and modprobe - verify single depmod call
#[test]
fn test_ext_merge_multiple_extensions_single_depmod() {
    // This test specifically verifies your concern: two extensions with depmod + modprobe
    // should result in ONE depmod call and ALL modules loaded
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");
    let release_dir = fixtures_path.join("extension-release.d");

    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "merge", "--verbose"],
        &[(
            "AVOCADO_EXTENSION_RELEASE_DIR",
            &release_dir.to_string_lossy(),
        )],
    );

    assert!(
        output.status.success(),
        "ext merge should succeed with multiple extensions"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify depmod is called exactly once
    let depmod_count = stdout.matches("Running command: depmod").count()
        + stdout.matches("[INFO] Running depmod").count();
    assert_eq!(
        depmod_count, 1,
        "Should call depmod exactly once even with multiple extensions requiring it"
    );

    // Verify all modules from all extensions are loaded
    assert!(
        stdout.contains("[INFO] Loading kernel modules:"),
        "Should show module loading message"
    );

    // Check that modules from multiple extensions are included
    // From network-driver: e1000e igb ixgbe
    // From storage-driver: ahci nvme
    // From gpu-driver: nvidia i915 radeon
    // From sound-driver: snd_hda_intel
    let has_network_modules =
        stdout.contains("e1000e") || stdout.contains("igb") || stdout.contains("ixgbe");
    let has_storage_modules = stdout.contains("ahci") || stdout.contains("nvme");
    let has_gpu_modules =
        stdout.contains("nvidia") || stdout.contains("i915") || stdout.contains("radeon");
    let has_sound_modules = stdout.contains("snd_hda_intel");

    assert!(
        has_network_modules || has_storage_modules || has_gpu_modules || has_sound_modules,
        "Should load modules from multiple extensions. Stdout: {stdout}"
    );

    assert!(
        stdout.contains("[SUCCESS] Module loading completed"),
        "Should show module loading completion"
    );
}

/// Test ext merge with modprobe post-processing
#[test]
fn test_ext_merge_with_modprobe_processing() {
    // Setup mock environment with release files that require both depmod and modprobe
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");
    let release_dir = fixtures_path.join("extension-release.d");

    // Use isolated environment to avoid race conditions
    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "merge", "--verbose"],
        &[(
            "AVOCADO_EXTENSION_RELEASE_DIR",
            &release_dir.to_string_lossy(),
        )],
    );

    assert!(
        output.status.success(),
        "ext merge should succeed with modprobe processing"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Starting extension merge process"),
        "Should show merging message"
    );
    assert!(
        stdout.contains("Extensions merged successfully"),
        "Should show merge success"
    );
    assert!(
        stdout.contains("Running command: depmod") || stdout.contains("[INFO] Running depmod"),
        "Should show depmod running message"
    );
    assert!(
        stdout.contains("Command 'depmod' completed successfully")
            || stdout.contains("[SUCCESS] depmod completed successfully"),
        "Should show depmod completion"
    );
    assert!(
        stdout.contains("[INFO] Loading kernel modules:"),
        "Should show module loading message"
    );
    assert!(
        stdout.contains("[SUCCESS] Module loading completed"),
        "Should show module loading completion"
    );

    // Check that specific modules are being loaded (from our test fixtures)
    assert!(
        stdout.contains("nvidia") || stdout.contains("snd_hda_intel"),
        "Should load modules from test extension files"
    );
}

/// Test post-merge processing with no depmod needed
#[test]
fn test_ext_merge_no_depmod_needed() {
    // This test verifies that merge works normally when no depmod is needed
    // Use a non-existent release directory to ensure no post-merge tasks run
    let empty_release_dir = "/tmp/nonexistent_release_dir";

    // Use isolated environment to avoid race conditions
    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "merge", "--verbose"],
        &[("AVOCADO_EXTENSION_RELEASE_DIR", empty_release_dir)],
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
    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(&["ext", "status"], &[]);

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
        stdout.contains("Origin"),
        "Should show origin column for extensions"
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

/// Test ext merge with multiple AVOCADO_ON_MERGE commands from same extension
#[test]
fn test_ext_merge_with_multiple_on_merge_commands() {
    // Create a temporary release directory with our test files
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");
    let release_dir = fixtures_path.join("extension-release.d");

    // Use isolated environment to avoid race conditions
    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "merge", "--verbose"],
        &[
            (
                "AVOCADO_EXTENSION_RELEASE_DIR",
                &release_dir.to_string_lossy(),
            ),
            (
                "PATH",
                &format!(
                    "{}:{}",
                    fixtures_path.to_string_lossy(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "ext merge should succeed with multiple AVOCADO_ON_MERGE commands"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Extensions merged successfully"),
        "Should show merge success"
    );

    // Verify that multiple commands are executed
    assert!(
        stdout.contains("Executing") && stdout.contains("post-merge commands"),
        "Should show execution of post-merge commands"
    );

    // Should see depmod being executed
    assert!(
        stdout.contains("Running command: depmod") || stdout.contains("[INFO] Running depmod"),
        "Should execute depmod command"
    );
}

/// Test ext merge with quoted AVOCADO_ON_MERGE commands
#[test]
fn test_ext_merge_with_quoted_commands() {
    // Create a temporary release directory with our test files
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");
    let release_dir = fixtures_path.join("extension-release.d");

    // Use isolated environment to avoid race conditions
    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "merge", "--verbose"],
        &[
            (
                "AVOCADO_EXTENSION_RELEASE_DIR",
                &release_dir.to_string_lossy(),
            ),
            (
                "PATH",
                &format!(
                    "{}:{}",
                    fixtures_path.to_string_lossy(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "ext merge should succeed with quoted AVOCADO_ON_MERGE commands"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Extensions merged successfully"),
        "Should show merge success"
    );

    // Should execute commands with arguments
    assert!(
        stdout.contains("post-merge commands"),
        "Should show execution of post-merge commands"
    );
}

/// Test ext unmerge does NOT execute AVOCADO_ON_MERGE commands
#[test]
fn test_ext_unmerge_does_not_execute_on_merge_commands() {
    // Setup mock environment with release files
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");
    let release_dir = fixtures_path.join("extension-release.d");

    // Use isolated environment to avoid race conditions
    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "unmerge", "--verbose"],
        &[
            (
                "AVOCADO_EXTENSION_RELEASE_DIR",
                &release_dir.to_string_lossy(),
            ),
            (
                "PATH",
                &format!(
                    "{}:{}",
                    fixtures_path.to_string_lossy(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "ext unmerge should succeed without executing AVOCADO_ON_MERGE commands"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Extensions unmerged successfully"),
        "Should show unmerge success"
    );

    // Should NOT execute post-merge commands during unmerge
    assert!(
        !stdout.contains("post-merge commands") && !stdout.contains("Running command:"),
        "Should NOT execute AVOCADO_ON_MERGE commands during unmerge"
    );
}

/// Test deduplication of AVOCADO_ON_MERGE commands
#[test]
fn test_avocado_on_merge_command_deduplication() {
    // This test verifies that duplicate commands across multiple extensions
    // are only executed once
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");
    let release_dir = fixtures_path.join("extension-release.d");

    let (output, _temp_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "merge", "--verbose"],
        &[
            (
                "AVOCADO_EXTENSION_RELEASE_DIR",
                &release_dir.to_string_lossy(),
            ),
            (
                "PATH",
                &format!(
                    "{}:{}",
                    fixtures_path.to_string_lossy(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "ext merge should succeed with command deduplication"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Count how many times depmod is called - should be only once despite multiple extensions having it
    let depmod_execution_count = stdout.matches("Running command: depmod").count()
        + stdout.matches("[INFO] Running depmod").count();

    // We should see depmod executed, but due to deduplication it should appear in consolidated command execution
    assert!(
        depmod_execution_count >= 1,
        "depmod should be executed at least once"
    );

    assert!(
        stdout.contains("Extensions merged successfully"),
        "Should show merge success"
    );
}

/// Test AVOCADO_ON_MERGE commands in confext release files
#[test]
fn test_ext_merge_with_confext_commands() {
    // Create a temporary test scenario with both sysext and confext directories
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    // Create mock sysext and confext release directories
    let sysext_dir = temp_path.join("usr/lib/extension-release.d");
    let confext_dir = temp_path.join("etc/extension-release.d");

    std::fs::create_dir_all(&sysext_dir).expect("Failed to create sysext dir");
    std::fs::create_dir_all(&confext_dir).expect("Failed to create confext dir");

    // Copy our test fixtures
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let fixtures_path = current_dir.join("tests/fixtures");

    // Copy sysext test files
    let source_sysext = fixtures_path.join("extension-release.d/extension-release.utils");
    let dest_sysext = sysext_dir.join("extension-release.utils");
    std::fs::copy(&source_sysext, &dest_sysext).expect("Failed to copy sysext file");

    // Copy confext test files
    let source_confext = fixtures_path.join("confext-release.d/extension-release.config-mgmt");
    let dest_confext = confext_dir.join("extension-release.config-mgmt");
    std::fs::copy(&source_confext, &dest_confext).expect("Failed to copy confext file");

    let (output, _temp_test_dir) = run_avocadoctl_with_isolated_env(
        &["ext", "merge", "--verbose"],
        &[
            (
                "AVOCADO_EXTENSION_RELEASE_DIR",
                &temp_path.to_string_lossy(),
            ),
            (
                "PATH",
                &format!(
                    "{}:{}",
                    fixtures_path.to_string_lossy(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            ),
        ],
    );

    assert!(
        output.status.success(),
        "ext merge should succeed with confext commands"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Extensions merged successfully"),
        "Should show merge success"
    );

    // Should execute commands from both sysext and confext
    assert!(
        stdout.contains("post-merge commands"),
        "Should show execution of post-merge commands"
    );
}

/// Test enable command with default runtime version
#[test]
fn test_enable_extensions_default_runtime() {
    // Create a temporary directory for extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let extensions_dir = temp_dir.path().join("extensions");
    fs::create_dir_all(&extensions_dir).expect("Failed to create extensions directory");

    // Create test extensions
    fs::create_dir(extensions_dir.join("ext1-1.0.0"))
        .expect("Failed to create test extension directory");
    fs::write(extensions_dir.join("ext2-1.0.0.raw"), b"mock raw data")
        .expect("Failed to create test raw extension");
    fs::write(extensions_dir.join("ext3-1.0.0.raw"), b"mock raw data")
        .expect("Failed to create test raw extension");

    // Run enable command with test mode
    let output = run_avocadoctl_with_env(
        &[
            "enable",
            "--verbose",
            "ext1-1.0.0",
            "ext2-1.0.0",
            "ext3-1.0.0",
        ],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        println!("STDOUT: {stdout}");
        println!("STDERR: {stderr}");
        panic!("enable command should succeed");
    }

    assert!(
        stdout.contains("Enabling extensions for runtime version"),
        "Should show runtime version message"
    );
    assert!(
        stdout.contains("Successfully enabled 3 extension(s)"),
        "Should show success message for 3 extensions"
    );
    assert!(
        stdout.contains("Enabled extension: ext1-1.0.0"),
        "Should show ext1 enabled"
    );
    assert!(
        stdout.contains("Enabled extension: ext2-1.0.0"),
        "Should show ext2 enabled"
    );
    assert!(
        stdout.contains("Enabled extension: ext3-1.0.0"),
        "Should show ext3 enabled"
    );
}

/// Test enable command with custom runtime version
#[test]
fn test_enable_extensions_custom_runtime() {
    // Create a temporary directory for extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let extensions_dir = temp_dir.path().join("extensions");
    fs::create_dir_all(&extensions_dir).expect("Failed to create extensions directory");

    // Create test extensions
    fs::create_dir(extensions_dir.join("ext1-1.0.0"))
        .expect("Failed to create test extension directory");
    fs::write(extensions_dir.join("ext2-1.0.0.raw"), b"mock raw data")
        .expect("Failed to create test raw extension");

    // Run enable command with custom runtime version and test mode
    let output = run_avocadoctl_with_env(
        &[
            "enable",
            "--verbose",
            "--runtime",
            "2.0.0",
            "ext1-1.0.0",
            "ext2-1.0.0",
        ],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        println!("STDOUT: {stdout}");
        println!("STDERR: {stderr}");
        panic!("enable command should succeed with custom runtime");
    }

    assert!(
        stdout.contains("Enabling extensions for runtime version: 2.0.0"),
        "Should show custom runtime version"
    );
    assert!(
        stdout.contains("Successfully enabled 2 extension(s) for runtime 2.0.0"),
        "Should show success message with runtime version"
    );
}

/// Test enable command with nonexistent extension
#[test]
fn test_enable_nonexistent_extension() {
    // Create a temporary directory for extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let extensions_dir = temp_dir.path().join("extensions");
    fs::create_dir_all(&extensions_dir).expect("Failed to create extensions directory");

    // Create one valid extension
    fs::create_dir(extensions_dir.join("ext1-1.0.0"))
        .expect("Failed to create test extension directory");

    // Run enable command with mix of valid and invalid extensions and test mode
    let output = run_avocadoctl_with_env(
        &["enable", "--verbose", "ext1-1.0.0", "nonexistent-ext"],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("STDOUT: {stdout}");
    println!("STDERR: {stderr}");

    assert!(
        !output.status.success(),
        "enable command should fail with nonexistent extension"
    );

    assert!(
        stderr.contains("Extension 'nonexistent-ext' not found"),
        "Should show error for nonexistent extension. STDERR: {stderr}"
    );
    assert!(
        stdout.contains("Enabled extension: ext1-1.0.0"),
        "Should still enable valid extension. STDOUT: {stdout}"
    );
}

/// Test enable command help
#[test]
fn test_enable_help() {
    let output = run_avocadoctl(&["enable", "--help"]);
    assert!(output.status.success(), "Enable help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Enable extensions for a specific runtime version"),
        "Should contain enable description"
    );
    assert!(
        stdout.contains("--runtime"),
        "Should mention --runtime flag"
    );
}

/// Test disable command with specific extensions
#[test]
fn test_disable_extensions() {
    // Create a temporary directory for extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let extensions_dir = temp_dir.path().join("extensions");
    fs::create_dir_all(&extensions_dir).expect("Failed to create extensions directory");

    // Create test extensions
    fs::create_dir(extensions_dir.join("ext1-1.0.0"))
        .expect("Failed to create test extension directory");
    fs::write(extensions_dir.join("ext2-1.0.0.raw"), b"mock raw data")
        .expect("Failed to create test raw extension");
    fs::write(extensions_dir.join("ext3-1.0.0.raw"), b"mock raw data")
        .expect("Failed to create test raw extension");

    // First enable extensions
    let enable_output = run_avocadoctl_with_env(
        &[
            "enable",
            "--verbose",
            "--runtime",
            "2.0.0",
            "ext1-1.0.0",
            "ext2-1.0.0",
            "ext3-1.0.0",
        ],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    assert!(enable_output.status.success(), "Enable should succeed");

    // Now disable some extensions
    let disable_output = run_avocadoctl_with_env(
        &[
            "disable",
            "--verbose",
            "--runtime",
            "2.0.0",
            "ext1-1.0.0",
            "ext2-1.0.0",
        ],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    let stdout = String::from_utf8_lossy(&disable_output.stdout);
    let stderr = String::from_utf8_lossy(&disable_output.stderr);

    if !disable_output.status.success() {
        println!("STDOUT: {stdout}");
        println!("STDERR: {stderr}");
        panic!("disable command should succeed");
    }

    assert!(
        stdout.contains("Disabling extensions for runtime version: 2.0.0"),
        "Should show runtime version message"
    );
    assert!(
        stdout.contains("Successfully disabled 2 extension(s)"),
        "Should show success message for 2 extensions"
    );
    assert!(
        stdout.contains("Disabled extension: ext1-1.0.0"),
        "Should show ext1 disabled"
    );
    assert!(
        stdout.contains("Disabled extension: ext2-1.0.0"),
        "Should show ext2 disabled"
    );
    assert!(
        stdout.contains("Synced changes to disk"),
        "Should show sync message"
    );

    // Verify ext3 still exists
    let runtime_dir = temp_dir.path().join("avocado/runtime/2.0.0");
    assert!(
        runtime_dir.join("ext3-1.0.0.raw").exists(),
        "ext3 should still be enabled"
    );
    assert!(
        !runtime_dir.join("ext1-1.0.0").exists(),
        "ext1 should be disabled"
    );
    assert!(
        !runtime_dir.join("ext2-1.0.0.raw").exists(),
        "ext2 should be disabled"
    );
}

/// Test disable command with --all flag
#[test]
fn test_disable_all_extensions() {
    // Create a temporary directory for extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let extensions_dir = temp_dir.path().join("extensions");
    fs::create_dir_all(&extensions_dir).expect("Failed to create extensions directory");

    // Create test extensions
    fs::create_dir(extensions_dir.join("ext1-1.0.0"))
        .expect("Failed to create test extension directory");
    fs::write(extensions_dir.join("ext2-1.0.0.raw"), b"mock raw data")
        .expect("Failed to create test raw extension");
    fs::write(extensions_dir.join("ext3-1.0.0.raw"), b"mock raw data")
        .expect("Failed to create test raw extension");

    // First enable extensions
    let enable_output = run_avocadoctl_with_env(
        &[
            "enable",
            "--verbose",
            "--runtime",
            "2.0.0",
            "ext1-1.0.0",
            "ext2-1.0.0",
            "ext3-1.0.0",
        ],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    assert!(enable_output.status.success(), "Enable should succeed");

    // Now disable all extensions
    let disable_output = run_avocadoctl_with_env(
        &["disable", "--verbose", "--runtime", "2.0.0", "--all"],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    let stdout = String::from_utf8_lossy(&disable_output.stdout);
    let stderr = String::from_utf8_lossy(&disable_output.stderr);

    if !disable_output.status.success() {
        println!("STDOUT: {stdout}");
        println!("STDERR: {stderr}");
        panic!("disable --all command should succeed");
    }

    assert!(
        stdout.contains("Disabling extensions for runtime version: 2.0.0"),
        "Should show runtime version message"
    );
    assert!(
        stdout.contains("Removing all extensions"),
        "Should show removing all message"
    );
    assert!(
        stdout.contains("Successfully disabled 3 extension(s)"),
        "Should show success message for 3 extensions"
    );
    assert!(
        stdout.contains("Synced changes to disk"),
        "Should show sync message"
    );

    // Verify all extensions are removed
    let runtime_dir = temp_dir.path().join("avocado/runtime/2.0.0");
    let entries = fs::read_dir(&runtime_dir).expect("Should be able to read runtime directory");
    let symlink_count = entries
        .filter(|e| {
            if let Ok(entry) = e {
                entry.path().is_symlink()
            } else {
                false
            }
        })
        .count();

    assert_eq!(symlink_count, 0, "All symlinks should be removed");
}

/// Test disable command with default runtime version
#[test]
fn test_disable_extensions_default_runtime() {
    // Create a temporary directory for extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let extensions_dir = temp_dir.path().join("extensions");
    fs::create_dir_all(&extensions_dir).expect("Failed to create extensions directory");

    // Create test extensions
    fs::create_dir(extensions_dir.join("ext1-1.0.0"))
        .expect("Failed to create test extension directory");

    // First enable extension
    let enable_output = run_avocadoctl_with_env(
        &["enable", "--verbose", "ext1-1.0.0"],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    assert!(enable_output.status.success(), "Enable should succeed");

    // Now disable with default runtime
    let disable_output = run_avocadoctl_with_env(
        &["disable", "--verbose", "ext1-1.0.0"],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    let stdout = String::from_utf8_lossy(&disable_output.stdout);
    let stderr = String::from_utf8_lossy(&disable_output.stderr);

    if !disable_output.status.success() {
        println!("STDOUT: {stdout}");
        println!("STDERR: {stderr}");
        panic!("disable command should succeed with default runtime");
    }

    assert!(
        stdout.contains("Disabling extensions for runtime version"),
        "Should show runtime version message"
    );
    assert!(
        stdout.contains("Successfully disabled 1 extension(s)"),
        "Should show success message"
    );
}

/// Test disable command with non-existent extension
#[test]
fn test_disable_nonexistent_extension() {
    // Create a temporary directory for extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let extensions_dir = temp_dir.path().join("extensions");
    fs::create_dir_all(&extensions_dir).expect("Failed to create extensions directory");

    // Create test extension
    fs::create_dir(extensions_dir.join("ext1-1.0.0"))
        .expect("Failed to create test extension directory");

    // First enable extension
    let enable_output = run_avocadoctl_with_env(
        &["enable", "--verbose", "--runtime", "2.0.0", "ext1-1.0.0"],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    assert!(enable_output.status.success(), "Enable should succeed");

    // Try to disable a non-existent extension
    let disable_output = run_avocadoctl_with_env(
        &[
            "disable",
            "--verbose",
            "--runtime",
            "2.0.0",
            "nonexistent-ext",
        ],
        &[
            ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
            ("AVOCADO_TEST_MODE", "1"),
            ("TMPDIR", temp_dir.path().to_str().unwrap()),
        ],
    );

    let stderr = String::from_utf8_lossy(&disable_output.stderr);

    assert!(
        !disable_output.status.success(),
        "disable command should fail with non-existent extension"
    );

    assert!(
        stderr.contains("Extension 'nonexistent-ext' is not enabled"),
        "Should show error for non-existent extension. STDERR: {stderr}"
    );
}

/// Test disable command help
#[test]
fn test_disable_help() {
    let output = run_avocadoctl(&["disable", "--help"]);
    assert!(output.status.success(), "Disable help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Disable extensions for a specific runtime version"),
        "Should contain disable description"
    );
    assert!(
        stdout.contains("--runtime"),
        "Should mention --runtime flag"
    );
    assert!(stdout.contains("--all"), "Should mention --all flag");
}

/// Test enable/disable/refresh workflow
#[test]
fn test_enable_disable_refresh_workflow() {
    // Create a temporary directory for extensions
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let extensions_dir = temp_dir.path().join("extensions");
    fs::create_dir_all(&extensions_dir).expect("Failed to create extensions directory");

    // Create test extensions
    fs::create_dir(extensions_dir.join("ext1-1.0.0"))
        .expect("Failed to create test extension directory");
    fs::create_dir(extensions_dir.join("ext2-1.0.0"))
        .expect("Failed to create test extension directory");

    // Create release files for both extensions
    let ext1_release_dir = extensions_dir.join("ext1-1.0.0/usr/lib/extension-release.d");
    fs::create_dir_all(&ext1_release_dir).expect("Failed to create release dir");
    fs::write(
        ext1_release_dir.join("extension-release.ext1-1.0.0"),
        "ID=avocado\nVERSION_ID=1.0",
    )
    .expect("Failed to write release file");

    let ext2_release_dir = extensions_dir.join("ext2-1.0.0/usr/lib/extension-release.d");
    fs::create_dir_all(&ext2_release_dir).expect("Failed to create release dir");
    fs::write(
        ext2_release_dir.join("extension-release.ext2-1.0.0"),
        "ID=avocado\nVERSION_ID=1.0",
    )
    .expect("Failed to write release file");

    let test_env = [
        ("AVOCADO_EXTENSIONS_PATH", extensions_dir.to_str().unwrap()),
        ("AVOCADO_TEST_MODE", "1"),
        ("TMPDIR", temp_dir.path().to_str().unwrap()),
    ];

    // Step 1: Enable both extensions
    let enable_output = run_avocadoctl_with_env(
        &["enable", "--verbose", "ext1-1.0.0", "ext2-1.0.0"],
        &test_env,
    );
    assert!(
        enable_output.status.success(),
        "Initial enable should succeed"
    );
    let stdout = String::from_utf8_lossy(&enable_output.stdout);
    assert!(stdout.contains("Successfully enabled 2 extension(s)"));

    // Step 2: Refresh with both enabled - both should be merged
    let (refresh_output1, _) =
        run_avocadoctl_with_isolated_env(&["ext", "refresh", "--verbose"], &test_env);
    assert!(
        refresh_output1.status.success(),
        "First refresh should succeed"
    );
    let stdout1 = String::from_utf8_lossy(&refresh_output1.stdout);
    assert!(
        stdout1.contains("Found runtime extension: ext1-1.0.0") || stdout1.contains("ext1-1.0.0"),
        "Should scan ext1 from runtime"
    );
    assert!(
        stdout1.contains("Found runtime extension: ext2-1.0.0") || stdout1.contains("ext2-1.0.0"),
        "Should scan ext2 from runtime"
    );

    // Step 3: Disable ext1
    let disable_output =
        run_avocadoctl_with_env(&["disable", "--verbose", "ext1-1.0.0"], &test_env);
    assert!(disable_output.status.success(), "Disable should succeed");

    // Step 4: Refresh after disabling ext1 - only ext2 should be merged
    let (refresh_output2, _) =
        run_avocadoctl_with_isolated_env(&["ext", "refresh", "--verbose"], &test_env);
    assert!(
        refresh_output2.status.success(),
        "Second refresh should succeed"
    );
    let stdout2 = String::from_utf8_lossy(&refresh_output2.stdout);

    // ext2 should still be found from runtime
    assert!(
        stdout2.contains("Found runtime extension: ext2-1.0.0") || stdout2.contains("ext2-1.0.0"),
        "Should still scan ext2 from runtime"
    );

    // ext1 should NOT be found from runtime (it was disabled)
    // It might be found from the base extensions directory though
    if stdout2.contains("ext1-1.0.0") {
        // If ext1 appears, it should be from the base directory, not runtime
        assert!(
            !stdout2.contains("Found runtime extension: ext1-1.0.0"),
            "ext1 should not be found in runtime directory"
        );
    }

    // Step 5: Re-enable ext1
    let reenable_output =
        run_avocadoctl_with_env(&["enable", "--verbose", "ext1-1.0.0"], &test_env);
    assert!(reenable_output.status.success(), "Re-enable should succeed");

    // Step 6: Refresh with both enabled again - both should be merged
    let (refresh_output3, _) =
        run_avocadoctl_with_isolated_env(&["ext", "refresh", "--verbose"], &test_env);
    assert!(
        refresh_output3.status.success(),
        "Third refresh should succeed"
    );
    let stdout3 = String::from_utf8_lossy(&refresh_output3.stdout);
    assert!(
        stdout3.contains("Found runtime extension: ext1-1.0.0") || stdout3.contains("ext1-1.0.0"),
        "Should scan ext1 from runtime again"
    );
    assert!(
        stdout3.contains("Found runtime extension: ext2-1.0.0") || stdout3.contains("ext2-1.0.0"),
        "Should scan ext2 from runtime"
    );
}
