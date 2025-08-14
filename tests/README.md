# Integration Tests

This directory contains integration tests for the `avocadoctl` CLI tool.

## Running Tests

### All Tests
```bash
cargo test
```

### Unit Tests Only
```bash
cargo test --lib
```

### Integration Tests Only
```bash
cargo test --test integration_tests
cargo test --test ext_integration_tests
```

### Specific Test
```bash
cargo test test_ext_list_with_mock_extensions
```

## Test Structure

### Unit Tests (`src/commands/ext.rs`)
- `test_environment_variable_precedence`: Tests that environment variables override default paths
- `test_default_path_when_no_env_var`: Tests default path behavior
- `test_extension_name_extraction`: Tests file name parsing logic
- `test_create_command`: Tests command structure creation

### General Integration Tests (`tests/integration_tests.rs`)
- `test_binary_exists`: Ensures the binary is built and accessible
- `test_version_command`: Tests `--version` flag functionality
- `test_help_command`: Tests `--help` flag functionality
- `test_default_behavior`: Tests default CLI behavior (no arguments)
- `test_invalid_command`: Tests error handling for invalid commands
- `test_cleanup_functionality`: Tests temporary file cleanup

### Extension Integration Tests (`tests/ext_integration_tests.rs`)
- `test_ext_help_shows_all_commands`: Tests `ext --help` shows all subcommands
- `test_ext_list_help`: Tests `ext list --help` command
- `test_ext_merge_help`: Tests `ext merge --help` command
- `test_ext_unmerge_help`: Tests `ext unmerge --help` command
- `test_ext_refresh_help`: Tests `ext refresh --help` command
- `test_ext_status_help`: Tests `ext status --help` command
- `test_ext_list_nonexistent_directory`: Tests error handling for missing directories
- `test_ext_list_with_mock_extensions`: Tests extension listing with mock data
- `test_ext_list_empty_directory`: Tests behavior with empty extensions directory
- `test_ext_merge_with_mocks`: Tests merge command with mock systemd binaries
- `test_ext_unmerge_with_mocks`: Tests unmerge command with mock systemd binaries
- `test_ext_refresh_with_mocks`: Tests refresh command (unmerge + merge) with mock systemd binaries
- `test_ext_status_with_mocks`: Tests status command with mock systemd binaries
- `test_ext_merge_with_depmod_processing`: Tests merge command with post-merge depmod processing
- `test_ext_merge_with_modprobe_processing`: Tests merge command with both depmod and modprobe processing
- `test_ext_merge_no_depmod_needed`: Tests merge command when no depmod is needed
- `test_example_config_fixture`: Tests example config file validation

### HITL Integration Tests (`tests/hitl_integration_tests.rs`)
- `test_hitl_help`: Tests `hitl --help` command
- `test_hitl_mount_help`: Tests `hitl mount --help` command
- `test_hitl_mount_with_mocks`: Tests mount command with mock NFS mounting
- `test_hitl_mount_short_options`: Tests mount command with short option flags
- `test_hitl_mount_default_port`: Tests mount command with default port
- `test_hitl_mount_missing_args`: Tests error handling for missing required arguments
- `test_main_help_shows_hitl`: Tests that main help shows hitl command

## Configuration Testing

Tests verify configuration file functionality including:
- TOML parsing of config files
- `-c` flag for custom config file paths
- Default config fallback when file doesn't exist
- Error handling for invalid TOML files
- Config precedence (environment variables override config)

## Environment Variables

Tests use the `AVOCADO_EXTENSIONS_PATH` environment variable to override the default extensions directory path for testing purposes. This takes precedence over configuration file settings.

## Temporary Files

Tests use the `tempfile` crate for proper temporary directory management. All temporary files and directories are automatically cleaned up after tests complete.

## Test Data

Integration tests create mock extension structures including:
- Directory-based extensions
- `.raw` file-based extensions
- Non-extension files (which should be ignored)
- Custom configuration files in TOML format

### Test Fixtures

The `tests/fixtures/` directory contains example files used for testing:
- `example_config.toml`: Sample configuration file demonstrating the TOML format
- `mock-systemd-sysext`: Mock systemd-sysext binary for testing merge/unmerge operations
- `mock-systemd-confext`: Mock systemd-confext binary for testing merge/unmerge operations
- `mock-depmod`: Mock depmod binary for testing post-merge processing
- `mock-modprobe`: Mock modprobe binary for testing module loading
- `mock-mount`: Mock mount binary for testing HITL NFS mounting
- `extension-release.d/`: Directory containing sample extension release files for testing post-merge processing

#### Mock Binaries

The mock binaries simulate the behavior of real system tools:
- `mock-systemd-sysext` and `mock-systemd-confext`: Support `merge` and `unmerge` actions with `--json=short` output format
- `mock-depmod`: Simulates kernel module dependency updates
- `mock-modprobe`: Simulates loading of kernel modules
- `mock-mount`: Simulates NFS mounting operations for HITL testing
- All mock binaries are activated when `AVOCADO_TEST_MODE` environment variable is set
- Return appropriate output for testing assertions

#### Extension Release Files

The `tests/fixtures/extension-release.d/` directory contains sample extension release files:
- `extension-release.nvidia-driver`: Contains `AVOCADO_ON_MERGE=depmod` to test depmod triggering
- `extension-release.app-bundle`: Contains no post-merge directives
- `extension-release.utils`: Contains `AVOCADO_ON_MERGE=other_command` to test non-depmod values
- `extension-release.gpu-driver`: Contains both `AVOCADO_ON_MERGE=depmod` and `AVOCADO_MODPROBE="nvidia i915 radeon"` to test combined functionality
- `extension-release.sound-driver`: Contains `AVOCADO_MODPROBE=snd_hda_intel` to test single module loading

Use the `AVOCADO_EXTENSION_RELEASE_DIR` environment variable to override the default `/usr/lib/extension-release.d` path for testing.

#### HITL Testing

The HITL (Hardware-in-the-loop) testing functionality allows mounting remote NFS extensions:
- Uses `mock-mount` binary in test mode to simulate NFS mounting
- Creates directories in the extensions path (configurable via `AVOCADO_EXTENSIONS_PATH`)
- Supports multiple extensions with customizable server IP and port
- Tests verify proper directory creation and mount command execution

#### depmod Behavior

The extension system automatically calls `depmod` to rebuild the kernel module dependency database:
- **After `ext merge`**: Always calls `depmod` if any extension release file contains `AVOCADO_ON_MERGE=depmod`
- **After `ext unmerge`**: Always calls `depmod` to clean up module dependencies
- **During `ext refresh`**: Calls `depmod` only once at the end (after merge), not after the unmerge phase

#### Module Loading (modprobe) Behavior

The extension system also supports automatic module loading via `modprobe`:
- **After `ext merge`**: Calls `modprobe` for each module listed in `AVOCADO_MODPROBE` from extension release files
- **Module loading order**: Modules are loaded **after** `depmod` completes successfully
- **Format**: `AVOCADO_MODPROBE="module1 module2 module3"` (space-separated list, with or without quotes)
- **Error handling**: Individual module loading failures are logged as warnings but don't fail the entire merge operation

The tests verify that:
- Only valid extensions are listed
- Extensions are sorted alphabetically
- File extensions (`.raw`) are stripped from display names
- Non-extension files are ignored
- Configuration files are parsed correctly
- `-c` flag overrides default config location
- Error conditions are handled gracefully
