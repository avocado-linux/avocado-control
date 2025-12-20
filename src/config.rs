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
    /// Mutability mode for system extensions - sysext (/usr, /opt)
    pub sysext_mutable: Option<String>,
    /// Mutability mode for configuration extensions - confext (/etc)
    pub confext_mutable: Option<String>,
    /// Legacy mutable option (deprecated, use sysext_mutable and confext_mutable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutable: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            avocado: AvocadoConfig {
                ext: ExtConfig {
                    dir: "/var/lib/avocado/extensions".to_string(),
                    sysext_mutable: None,
                    confext_mutable: None,
                    mutable: None,
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

    /// Get the sysext mutable mode, defaulting to "ephemeral" if not set
    /// Validates that the value is one of the supported systemd options
    pub fn get_sysext_mutable(&self) -> Result<String, ConfigError> {
        // Priority: sysext_mutable > legacy mutable > default
        let value = self
            .avocado
            .ext
            .sysext_mutable
            .as_ref()
            .or(self.avocado.ext.mutable.as_ref())
            .unwrap_or(&"ephemeral".to_string())
            .clone();

        // Validate against supported systemd options
        match value.as_str() {
            "no" | "auto" | "yes" | "import" | "ephemeral" | "ephemeral-import" => Ok(value),
            _ => Err(ConfigError::InvalidMutableValue { value }),
        }
    }

    /// Get the confext mutable mode, defaulting to "ephemeral" if not set
    /// Validates that the value is one of the supported systemd options
    pub fn get_confext_mutable(&self) -> Result<String, ConfigError> {
        // Priority: confext_mutable > legacy mutable > default
        let value = self
            .avocado
            .ext
            .confext_mutable
            .as_ref()
            .or(self.avocado.ext.mutable.as_ref())
            .unwrap_or(&"ephemeral".to_string())
            .clone();

        // Validate against supported systemd options
        match value.as_str() {
            "no" | "auto" | "yes" | "import" | "ephemeral" | "ephemeral-import" => Ok(value),
            _ => Err(ConfigError::InvalidMutableValue { value }),
        }
    }

    /// Legacy method for backward compatibility
    /// Get the extension mutable mode, defaulting to "ephemeral" if not set
    /// Validates that the value is one of the supported systemd options
    #[deprecated(note = "Use get_sysext_mutable() and get_confext_mutable() instead")]
    #[allow(dead_code)]
    pub fn get_extension_mutable(&self) -> Result<String, ConfigError> {
        // For backward compatibility, return sysext_mutable if available, otherwise legacy mutable
        self.get_sysext_mutable()
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

    #[error("Invalid mutable value '{value}'. Must be one of: no, auto, yes, import, ephemeral, ephemeral-import")]
    InvalidMutableValue { value: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Mutex to serialize tests that modify AVOCADO_EXTENSIONS_PATH environment variable
    static ENV_VAR_MUTEX: Mutex<()> = Mutex::new(());

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
        // Lock the mutex to prevent env var interference from other tests
        let _guard = ENV_VAR_MUTEX.lock().unwrap();

        // Save original environment variable value for restoration
        let original_value = std::env::var("AVOCADO_EXTENSIONS_PATH").ok();

        let config = Config::default();

        // Test without environment variable
        std::env::remove_var("AVOCADO_EXTENSIONS_PATH");
        assert_eq!(config.get_extensions_dir(), "/var/lib/avocado/extensions");

        // Test with environment variable
        std::env::set_var("AVOCADO_EXTENSIONS_PATH", "/env/override/path");
        assert_eq!(config.get_extensions_dir(), "/env/override/path");

        // Restore original environment variable
        match original_value {
            Some(val) => std::env::set_var("AVOCADO_EXTENSIONS_PATH", val),
            None => std::env::remove_var("AVOCADO_EXTENSIONS_PATH"),
        }
    }

    #[test]
    fn test_get_sysext_mutable() {
        // Test default value
        let config = Config::default();
        assert_eq!(config.get_sysext_mutable().unwrap(), "ephemeral");

        // Test with valid custom values
        let valid_values = [
            "no",
            "auto",
            "yes",
            "import",
            "ephemeral",
            "ephemeral-import",
        ];
        for value in valid_values {
            let mut config = Config::default();
            config.avocado.ext.sysext_mutable = Some(value.to_string());
            assert_eq!(config.get_sysext_mutable().unwrap(), value);
        }

        // Test with invalid value
        let mut config = Config::default();
        config.avocado.ext.sysext_mutable = Some("invalid".to_string());
        assert!(config.get_sysext_mutable().is_err());
    }

    #[test]
    fn test_get_confext_mutable() {
        // Test default value
        let config = Config::default();
        assert_eq!(config.get_confext_mutable().unwrap(), "ephemeral");

        // Test with valid custom values
        let valid_values = [
            "no",
            "auto",
            "yes",
            "import",
            "ephemeral",
            "ephemeral-import",
        ];
        for value in valid_values {
            let mut config = Config::default();
            config.avocado.ext.confext_mutable = Some(value.to_string());
            assert_eq!(config.get_confext_mutable().unwrap(), value);
        }

        // Test with invalid value
        let mut config = Config::default();
        config.avocado.ext.confext_mutable = Some("invalid".to_string());
        assert!(config.get_confext_mutable().is_err());
    }

    #[test]
    fn test_backward_compatibility_mutable() {
        // Test that legacy mutable option works for both sysext and confext
        let mut config = Config::default();
        config.avocado.ext.mutable = Some("yes".to_string());

        // Both should fall back to legacy mutable value
        assert_eq!(config.get_sysext_mutable().unwrap(), "yes");
        assert_eq!(config.get_confext_mutable().unwrap(), "yes");

        // Test priority: specific options override legacy
        config.avocado.ext.sysext_mutable = Some("auto".to_string());
        config.avocado.ext.confext_mutable = Some("no".to_string());

        assert_eq!(config.get_sysext_mutable().unwrap(), "auto");
        assert_eq!(config.get_confext_mutable().unwrap(), "no");
    }

    #[test]
    fn test_get_extension_mutable() {
        // Test legacy method for backward compatibility
        let config = Config::default();
        #[allow(deprecated)]
        {
            assert_eq!(config.get_extension_mutable().unwrap(), "ephemeral");
        }

        // Test with valid custom values
        let valid_values = [
            "no",
            "auto",
            "yes",
            "import",
            "ephemeral",
            "ephemeral-import",
        ];
        for value in valid_values {
            let mut config = Config::default();
            config.avocado.ext.mutable = Some(value.to_string());
            #[allow(deprecated)]
            {
                assert_eq!(config.get_extension_mutable().unwrap(), value);
            }
        }

        // Test with invalid value
        let mut config = Config::default();
        config.avocado.ext.mutable = Some("invalid".to_string());
        #[allow(deprecated)]
        {
            assert!(config.get_extension_mutable().is_err());
        }
    }

    #[test]
    fn test_load_config_with_separate_mutable_options() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("separate_mutable_test.toml");

        let config_content = r#"
[avocado.ext]
dir = "/test/extensions"
sysext_mutable = "yes"
confext_mutable = "auto"
"#;

        fs::write(&config_path, config_content).unwrap();

        let config = Config::load(&config_path).unwrap();
        assert_eq!(config.avocado.ext.dir, "/test/extensions");
        assert_eq!(config.get_sysext_mutable().unwrap(), "yes");
        assert_eq!(config.get_confext_mutable().unwrap(), "auto");
    }

    #[test]
    fn test_load_config_with_mutable_option() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("mutable_test.toml");

        let config_content = r#"
[avocado.ext]
dir = "/test/extensions"
mutable = "yes"
"#;

        fs::write(&config_path, config_content).unwrap();

        let config = Config::load(&config_path).unwrap();
        assert_eq!(config.avocado.ext.dir, "/test/extensions");
        #[allow(deprecated)]
        {
            assert_eq!(config.get_extension_mutable().unwrap(), "yes");
        }
        // Legacy mutable should apply to both
        assert_eq!(config.get_sysext_mutable().unwrap(), "yes");
        assert_eq!(config.get_confext_mutable().unwrap(), "yes");
    }

    #[test]
    fn test_save_and_load_config_with_separate_mutable() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir
            .path()
            .join("roundtrip_separate_mutable_config.toml");

        let mut config = Config::default();
        config.avocado.ext.dir = "/test/extensions".to_string();
        config.avocado.ext.sysext_mutable = Some("auto".to_string());
        config.avocado.ext.confext_mutable = Some("yes".to_string());

        config.save(&config_path).unwrap();

        let loaded_config = Config::load(&config_path).unwrap();
        assert_eq!(loaded_config.avocado.ext.dir, "/test/extensions");
        assert_eq!(loaded_config.get_sysext_mutable().unwrap(), "auto");
        assert_eq!(loaded_config.get_confext_mutable().unwrap(), "yes");
    }

    #[test]
    fn test_save_and_load_config_with_mutable() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("roundtrip_mutable_config.toml");

        let mut config = Config::default();
        config.avocado.ext.dir = "/test/extensions".to_string();
        config.avocado.ext.mutable = Some("auto".to_string());

        config.save(&config_path).unwrap();

        let loaded_config = Config::load(&config_path).unwrap();
        assert_eq!(loaded_config.avocado.ext.dir, "/test/extensions");
        #[allow(deprecated)]
        {
            assert_eq!(loaded_config.get_extension_mutable().unwrap(), "auto");
        }
    }

    #[test]
    fn test_mutable_validation_error_message() {
        // Test sysext validation error
        let mut config = Config::default();
        config.avocado.ext.sysext_mutable = Some("invalid_value".to_string());

        let result = config.get_sysext_mutable();
        assert!(result.is_err());

        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("Invalid mutable value 'invalid_value'"));
        assert!(error_message
            .contains("Must be one of: no, auto, yes, import, ephemeral, ephemeral-import"));

        // Test confext validation error
        let mut config = Config::default();
        config.avocado.ext.confext_mutable = Some("invalid_value".to_string());

        let result = config.get_confext_mutable();
        assert!(result.is_err());

        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("Invalid mutable value 'invalid_value'"));
        assert!(error_message
            .contains("Must be one of: no, auto, yes, import, ephemeral, ephemeral-import"));

        // Test legacy validation error
        let mut config = Config::default();
        config.avocado.ext.mutable = Some("invalid_value".to_string());

        #[allow(deprecated)]
        let result = config.get_extension_mutable();
        assert!(result.is_err());

        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("Invalid mutable value 'invalid_value'"));
        assert!(error_message
            .contains("Must be one of: no, auto, yes, import, ephemeral, ephemeral-import"));
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
