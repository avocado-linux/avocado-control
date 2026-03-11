use serde::{Deserialize, Serialize};

/// Extension information returned by the service layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionInfo {
    pub name: String,
    pub version: Option<String>,
    pub path: String,
    pub is_sysext: bool,
    pub is_confext: bool,
    pub is_directory: bool,
}

/// Result of an enable operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnableResult {
    pub enabled: usize,
    pub failed: usize,
}

/// Result of a disable operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisableResult {
    pub disabled: usize,
    pub failed: usize,
}

/// Runtime summary for status display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSummary {
    pub name: Option<String>,
    pub version: Option<String>,
    pub id: Option<String>,
    pub built_at: Option<String>,
    pub manifest_version: Option<u32>,
}

/// Runtime entry for list/inspect
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEntry {
    pub id: String,
    pub manifest_version: u32,
    pub built_at: String,
    pub name: String,
    pub version: String,
    pub extensions: Vec<RuntimeExtensionInfo>,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_build_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initramfs_build_id: Option<String>,
}

/// Extension info within a runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeExtensionInfo {
    pub name: String,
    pub version: String,
    pub image_id: Option<String>,
}

/// Root authority information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootAuthorityInfo {
    pub version: u64,
    pub expires: String,
    pub keys: Vec<TrustedKey>,
}

/// A trusted signing key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedKey {
    pub key_id: String,
    pub key_type: String,
    pub roles: Vec<String>,
}
