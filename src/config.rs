use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Default configuration file path
pub const DEFAULT_CONFIG_PATH: &str = "/etc/avocado/avocadoctl.conf";

/// Configuration structure for avocadoctl
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Avocado extension configuration
    pub avocado: AvocadoConfig,
}

/// Avocado-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvocadoConfig {
    /// Extension configuration
    pub ext: ExtConfig,
}

/// Extension configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtConfig {
    /// Directory where extensions are stored
    pub dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            avocado: AvocadoConfig {
                ext: ExtConfig {
                    dir: "/var/lib/avocado/extensions".to_string(),
                },
            },
        }
    }
}

impl Config {
    /// Load configuration from file, falling back to defaults if file doesn't exist
    pub fn load<P: AsRef<Path>>(config_path: P) -> Result<Self, ConfigError> {
        let path = config_path.as_ref();

        if !path.exists() {
            // Return default config if file doesn't exist
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path).map_err(|e| ConfigError::FileRead {
            path: path.to_path_buf(),
            source: e,
        })?;

        let config: Config = toml::from_str(&content).map_err(|e| ConfigError::Parse {
            path: path.to_path_buf(),
            source: e,
        })?;

        Ok(config)
    }

    /// Load configuration from the default path or a custom path
    pub fn load_with_override(custom_path: Option<&str>) -> Result<Self, ConfigError> {
        let config_path = custom_path.unwrap_or(DEFAULT_CONFIG_PATH);
        Self::load(config_path)
    }

    /// Get the extensions directory, checking environment variable first
    pub fn get_extensions_dir(&self) -> String {
        // Environment variable takes precedence (for testing)
        std::env::var("AVOCADO_EXTENSIONS_PATH").unwrap_or_else(|_| self.avocado.ext.dir.clone())
    }

    /// Save configuration to file (mainly for testing)
    #[cfg(test)]
    pub fn save<P: AsRef<Path>>(&self, config_path: P) -> Result<(), ConfigError> {
        let path = config_path.as_ref();
        let content =
            toml::to_string_pretty(self).map_err(|e| ConfigError::Serialize { source: e })?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| ConfigError::FileWrite {
                path: path.to_path_buf(),
                source: e,
            })?;
        }

        fs::write(path, content).map_err(|e| ConfigError::FileWrite {
            path: path.to_path_buf(),
            source: e,
        })?;

        Ok(())
    }
}

/// Configuration-related errors
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum ConfigError {
    #[error("Failed to read config file '{path}': {source}")]
    FileRead {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to write config file '{path}': {source}")]
    FileWrite {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse config file '{path}': {source}")]
    Parse {
        path: std::path::PathBuf,
        source: toml::de::Error,
    },

    #[error("Failed to serialize config: {source}")]
    Serialize { source: toml::ser::Error },
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.avocado.ext.dir, "/var/lib/avocado/extensions");
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = Config::load("/nonexistent/path/config.toml");
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.avocado.ext.dir, "/var/lib/avocado/extensions");
    }

    #[test]
    fn test_load_valid_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");

        let config_content = r#"
[avocado.ext]
dir = "/custom/extensions/path"
"#;

        fs::write(&config_path, config_content).unwrap();

        let config = Config::load(&config_path).unwrap();
        assert_eq!(config.avocado.ext.dir, "/custom/extensions/path");
    }

    #[test]
    fn test_load_invalid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid_config.toml");

        fs::write(&config_path, "invalid toml content [[[").unwrap();

        let result = Config::load(&config_path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::Parse { .. }));
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("roundtrip_config.toml");

        let mut config = Config::default();
        config.avocado.ext.dir = "/test/extensions".to_string();

        config.save(&config_path).unwrap();

        let loaded_config = Config::load(&config_path).unwrap();
        assert_eq!(loaded_config.avocado.ext.dir, "/test/extensions");
    }

    #[test]
    fn test_get_extensions_dir_with_env_var() {
        let config = Config::default();

        // Test without environment variable
        std::env::remove_var("AVOCADO_EXTENSIONS_PATH");
        assert_eq!(config.get_extensions_dir(), "/var/lib/avocado/extensions");

        // Test with environment variable
        std::env::set_var("AVOCADO_EXTENSIONS_PATH", "/env/override/path");
        assert_eq!(config.get_extensions_dir(), "/env/override/path");

        // Clean up
        std::env::remove_var("AVOCADO_EXTENSIONS_PATH");
    }

    #[test]
    fn test_load_with_override() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("override_test.toml");

        let config_content = r#"
[avocado.ext]
dir = "/override/test/path"
"#;

        fs::write(&config_path, config_content).unwrap();

        // Test with custom path
        let config = Config::load_with_override(Some(config_path.to_str().unwrap())).unwrap();
        assert_eq!(config.avocado.ext.dir, "/override/test/path");

        // Test with default path (should return default config since default doesn't exist)
        let default_config = Config::load_with_override(None).unwrap();
        assert_eq!(
            default_config.avocado.ext.dir,
            "/var/lib/avocado/extensions"
        );
    }
}
