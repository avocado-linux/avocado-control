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
- `test_ext_help`: Tests `ext --help` subcommand
- `test_ext_list_help`: Tests `ext list --help` command
- `test_ext_list_nonexistent_directory`: Tests error handling for missing directories
- `test_ext_list_with_mock_extensions`: Tests extension listing with mock data
- `test_ext_list_empty_directory`: Tests behavior with empty extensions directory

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

The tests verify that:
- Only valid extensions are listed
- Extensions are sorted alphabetically
- File extensions (`.raw`) are stripped from display names
- Non-extension files are ignored
- Configuration files are parsed correctly
- `-c` flag overrides default config location
- Error conditions are handled gracefully
