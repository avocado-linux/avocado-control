use crate::config::Config;
use clap::{ArgMatches, Command};
use std::fs;

/// Create the ext subcommand definition
pub fn create_command() -> Command {
    Command::new("ext")
        .about("Extension management commands")
        .subcommand(Command::new("list").about("List all available extensions"))
}

/// Handle ext command and its subcommands
pub fn handle_command(matches: &ArgMatches, config: &Config) {
    match matches.subcommand() {
        Some(("list", _)) => {
            list_extensions(config);
        }
        _ => {
            println!("Use 'avocadoctl ext --help' for available extension commands");
        }
    }
}

/// List all extensions from the extensions directory
fn list_extensions(config: &Config) {
    let extensions_path = config.get_extensions_dir();

    match fs::read_dir(&extensions_path) {
        Ok(entries) => {
            let mut extension_names = Vec::new();

            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if let Some(name) = path.file_name() {
                            if let Some(name_str) = name.to_str() {
                                // Handle directories and .raw files
                                if path.is_dir() {
                                    extension_names.push(name_str.to_string());
                                } else if name_str.ends_with(".raw") {
                                    // Remove .raw extension from filename
                                    let ext_name =
                                        name_str.strip_suffix(".raw").unwrap_or(name_str);
                                    extension_names.push(ext_name.to_string());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading entry: {e}");
                    }
                }
            }

            if extension_names.is_empty() {
                println!("No extensions found in {extensions_path}");
            } else {
                extension_names.sort();
                println!("Available extensions:");
                for name in extension_names {
                    println!("  {name}");
                }
            }
        }
        Err(e) => {
            eprintln!("Error accessing extensions directory '{extensions_path}': {e}");
            eprintln!("Make sure the directory exists and you have read permissions.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::env;

    #[test]
    fn test_config_integration() {
        // Test that config is used for extensions directory
        let mut config = Config::default();
        config.avocado.ext.dir = "/test/config/path".to_string();

        let extensions_path = config.get_extensions_dir();
        assert_eq!(extensions_path, "/test/config/path");
    }

    #[test]
    fn test_environment_variable_precedence() {
        // Test that environment variable overrides config
        let mut config = Config::default();
        config.avocado.ext.dir = "/config/path".to_string();

        env::set_var("AVOCADO_EXTENSIONS_PATH", "/env/override/path");
        let extensions_path = config.get_extensions_dir();
        assert_eq!(extensions_path, "/env/override/path");

        // Clean up
        env::remove_var("AVOCADO_EXTENSIONS_PATH");

        // Now should use config value
        let extensions_path = config.get_extensions_dir();
        assert_eq!(extensions_path, "/config/path");
    }

    #[test]
    fn test_default_path_when_no_config_or_env() {
        // Ensure no environment variable is set
        env::remove_var("AVOCADO_EXTENSIONS_PATH");

        let config = Config::default();
        let extensions_path = config.get_extensions_dir();
        assert_eq!(extensions_path, "/var/lib/avocado/extensions");
    }

    #[test]
    fn test_extension_name_extraction() {
        // Test file name extraction logic
        use std::path::Path;

        // Test directory name
        let dir_path = Path::new("/test/path/my_extension");
        if let Some(name) = dir_path.file_name() {
            if let Some(name_str) = name.to_str() {
                assert_eq!(name_str, "my_extension");
            }
        }

        // Test .raw file name
        let raw_path = Path::new("/test/path/my_extension.raw");
        if let Some(name) = raw_path.file_name() {
            if let Some(name_str) = name.to_str() {
                if name_str.ends_with(".raw") {
                    let ext_name = name_str.strip_suffix(".raw").unwrap_or(name_str);
                    assert_eq!(ext_name, "my_extension");
                }
            }
        }
    }

    #[test]
    fn test_create_command() {
        let cmd = create_command();
        assert_eq!(cmd.get_name(), "ext");

        // Check that list subcommand exists
        let subcommands: Vec<_> = cmd.get_subcommands().collect();
        assert_eq!(subcommands.len(), 1);
        assert_eq!(subcommands[0].get_name(), "list");
    }
}
