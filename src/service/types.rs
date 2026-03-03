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

/// Extension status as reported by systemd
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionStatus {
    pub name: String,
    pub version: Option<String>,
    pub is_sysext: bool,
    pub is_confext: bool,
    pub is_merged: bool,
    pub origin: Option<String>,
    pub image_id: Option<String>,
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

/// Full status result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResult {
    pub runtime: Option<RuntimeSummary>,
    pub extensions: Vec<ExtensionStatus>,
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
