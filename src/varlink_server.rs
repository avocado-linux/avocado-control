#![allow(non_snake_case)]

use crate::config::Config;
use crate::service;
use crate::service::error::AvocadoError;
use crate::varlink::{
    org_avocado_Extensions as vl_ext, org_avocado_Hitl as vl_hitl,
    org_avocado_RootAuthority as vl_ra, org_avocado_Runtimes as vl_rt,
};

// ── Extensions handler ──────────────────────────────────────────────

pub struct ExtensionsHandler {
    config: Config,
}

macro_rules! map_ext_error {
    ($call:expr, $err:expr) => {
        match $err {
            AvocadoError::ExtensionNotFound { name } => $call.reply_extension_not_found(name),
            AvocadoError::MergeFailed { reason } => $call.reply_merge_failed(reason),
            AvocadoError::UnmergeFailed { reason } => $call.reply_unmerge_failed(reason),
            AvocadoError::ConfigurationError { message } => {
                $call.reply_configuration_error(message)
            }
            e => $call.reply_command_failed("avocadoctl".to_string(), e.to_string()),
        }
    };
}

impl vl_ext::VarlinkInterface for ExtensionsHandler {
    fn list(&self, call: &mut dyn vl_ext::Call_List) -> varlink::Result<()> {
        match service::ext::list_extensions(&self.config) {
            Ok(extensions) => {
                let vl: Vec<vl_ext::Extension> = extensions
                    .into_iter()
                    .map(|e| vl_ext::Extension {
                        r#name: e.name,
                        r#version: e.version,
                        r#path: e.path,
                        r#isSysext: e.is_sysext,
                        r#isConfext: e.is_confext,
                        r#isDirectory: e.is_directory,
                    })
                    .collect();
                call.reply(vl)
            }
            Err(e) => map_ext_error!(call, e),
        }
    }

    fn merge(&self, call: &mut dyn vl_ext::Call_Merge) -> varlink::Result<()> {
        match service::ext::merge_extensions(&self.config) {
            Ok(()) => call.reply(),
            Err(e) => map_ext_error!(call, e),
        }
    }

    fn unmerge(
        &self,
        call: &mut dyn vl_ext::Call_Unmerge,
        r#unmount: Option<bool>,
    ) -> varlink::Result<()> {
        match service::ext::unmerge_extensions(unmount.unwrap_or(false)) {
            Ok(()) => call.reply(),
            Err(e) => map_ext_error!(call, e),
        }
    }

    fn refresh(&self, call: &mut dyn vl_ext::Call_Refresh) -> varlink::Result<()> {
        match service::ext::refresh_extensions(&self.config) {
            Ok(()) => call.reply(),
            Err(e) => map_ext_error!(call, e),
        }
    }

    fn enable(
        &self,
        call: &mut dyn vl_ext::Call_Enable,
        r#extensions: Vec<String>,
        r#osRelease: Option<String>,
    ) -> varlink::Result<()> {
        let ext_refs: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
        match service::ext::enable_extensions(osRelease.as_deref(), &ext_refs, &self.config) {
            Ok(result) => call.reply(result.enabled as i64, result.failed as i64),
            Err(e) => map_ext_error!(call, e),
        }
    }

    fn disable(
        &self,
        call: &mut dyn vl_ext::Call_Disable,
        r#extensions: Option<Vec<String>>,
        r#all: Option<bool>,
        r#osRelease: Option<String>,
    ) -> varlink::Result<()> {
        let ext_refs: Option<Vec<&str>> = extensions
            .as_ref()
            .map(|v| v.iter().map(|s| s.as_str()).collect());
        match service::ext::disable_extensions(
            osRelease.as_deref(),
            ext_refs.as_deref(),
            all.unwrap_or(false),
        ) {
            Ok(result) => call.reply(result.disabled as i64, result.failed as i64),
            Err(e) => map_ext_error!(call, e),
        }
    }

    fn status(&self, call: &mut dyn vl_ext::Call_Status) -> varlink::Result<()> {
        match service::ext::status_extensions(&self.config) {
            Ok(extensions) => call.reply(extensions),
            Err(e) => map_ext_error!(call, e),
        }
    }
}

// ── Runtimes handler ────────────────────────────────────────────────

pub struct RuntimesHandler {
    config: Config,
}

macro_rules! map_rt_error {
    ($call:expr, $err:expr) => {
        match $err {
            AvocadoError::RuntimeNotFound { id } => $call.reply_runtime_not_found(id),
            AvocadoError::AmbiguousRuntimeId { id, candidates } => {
                $call.reply_ambiguous_runtime_id(id, candidates)
            }
            AvocadoError::RemoveActiveRuntime => $call.reply_remove_active_runtime(),
            AvocadoError::StagingFailed { reason } => $call.reply_staging_failed(reason),
            AvocadoError::UpdateFailed { reason } => $call.reply_update_failed(reason),
            e => $call.reply_staging_failed(e.to_string()),
        }
    };
}

fn runtime_entry_to_varlink(entry: crate::service::types::RuntimeEntry) -> vl_rt::Runtime {
    vl_rt::Runtime {
        r#id: entry.id,
        r#manifestVersion: entry.manifest_version as i64,
        r#builtAt: entry.built_at,
        r#runtime: vl_rt::RuntimeInfo {
            r#name: entry.name,
            r#version: entry.version,
        },
        r#extensions: entry
            .extensions
            .into_iter()
            .map(|e| vl_rt::ManifestExtension {
                r#name: e.name,
                r#version: e.version,
                r#imageId: e.image_id,
            })
            .collect(),
        r#active: entry.active,
    }
}

impl vl_rt::VarlinkInterface for RuntimesHandler {
    fn list(&self, call: &mut dyn vl_rt::Call_List) -> varlink::Result<()> {
        match service::runtime::list_runtimes(&self.config) {
            Ok(runtimes) => {
                let vl: Vec<vl_rt::Runtime> =
                    runtimes.into_iter().map(runtime_entry_to_varlink).collect();
                call.reply(vl)
            }
            Err(e) => map_rt_error!(call, e),
        }
    }

    fn add_from_url(
        &self,
        call: &mut dyn vl_rt::Call_AddFromUrl,
        r#url: String,
    ) -> varlink::Result<()> {
        match service::runtime::add_from_url(&url, &self.config) {
            Ok(()) => call.reply(),
            Err(e) => map_rt_error!(call, e),
        }
    }

    fn add_from_manifest(
        &self,
        call: &mut dyn vl_rt::Call_AddFromManifest,
        r#manifestPath: String,
    ) -> varlink::Result<()> {
        match service::runtime::add_from_manifest(&manifestPath, &self.config) {
            Ok(()) => call.reply(),
            Err(e) => map_rt_error!(call, e),
        }
    }

    fn remove(&self, call: &mut dyn vl_rt::Call_Remove, r#id: String) -> varlink::Result<()> {
        match service::runtime::remove_runtime(&id, &self.config) {
            Ok(()) => call.reply(),
            Err(e) => map_rt_error!(call, e),
        }
    }

    fn activate(&self, call: &mut dyn vl_rt::Call_Activate, r#id: String) -> varlink::Result<()> {
        match service::runtime::activate_runtime(&id, &self.config) {
            Ok(()) => call.reply(),
            Err(e) => map_rt_error!(call, e),
        }
    }

    fn inspect(&self, call: &mut dyn vl_rt::Call_Inspect, r#id: String) -> varlink::Result<()> {
        match service::runtime::inspect_runtime(&id, &self.config) {
            Ok(entry) => call.reply(runtime_entry_to_varlink(entry)),
            Err(e) => map_rt_error!(call, e),
        }
    }
}

// ── HITL handler ────────────────────────────────────────────────────

pub struct HitlHandler;

macro_rules! map_hitl_error {
    ($call:expr, $err:expr) => {
        match $err {
            AvocadoError::MountFailed { extension, reason } => {
                $call.reply_mount_failed(extension, reason)
            }
            AvocadoError::UnmountFailed { extension, reason } => {
                $call.reply_unmount_failed(extension, reason)
            }
            e => $call.reply_mount_failed("unknown".to_string(), e.to_string()),
        }
    };
}

impl vl_hitl::VarlinkInterface for HitlHandler {
    fn mount(
        &self,
        call: &mut dyn vl_hitl::Call_Mount,
        r#serverIp: String,
        r#serverPort: Option<String>,
        r#extensions: Vec<String>,
    ) -> varlink::Result<()> {
        match service::hitl::mount(&serverIp, serverPort.as_deref(), &extensions) {
            Ok(()) => call.reply(),
            Err(e) => map_hitl_error!(call, e),
        }
    }

    fn unmount(
        &self,
        call: &mut dyn vl_hitl::Call_Unmount,
        r#extensions: Vec<String>,
    ) -> varlink::Result<()> {
        match service::hitl::unmount(&extensions) {
            Ok(()) => call.reply(),
            Err(e) => map_hitl_error!(call, e),
        }
    }
}

// ── Root Authority handler ──────────────────────────────────────────

pub struct RootAuthorityHandler {
    config: Config,
}

impl vl_ra::VarlinkInterface for RootAuthorityHandler {
    fn show(&self, call: &mut dyn vl_ra::Call_Show) -> varlink::Result<()> {
        match service::root_authority::show(&self.config) {
            Ok(Some(info)) => {
                let vl_info = vl_ra::RootAuthorityInfo {
                    r#version: info.version as i64,
                    r#expires: info.expires,
                    r#keys: info
                        .keys
                        .into_iter()
                        .map(|k| vl_ra::TrustedKey {
                            r#keyId: k.key_id,
                            r#keyType: k.key_type,
                            r#roles: k.roles,
                        })
                        .collect(),
                };
                call.reply(Some(vl_info))
            }
            Ok(None) => call.reply(None),
            Err(AvocadoError::NoRootAuthority) => call.reply_no_root_authority(),
            Err(AvocadoError::ParseFailed { reason }) => call.reply_parse_failed(reason),
            Err(e) => call.reply_parse_failed(e.to_string()),
        }
    }
}

// ── Server entry point ──────────────────────────────────────────────

pub fn run_server(address: &str, config: Config) -> varlink::Result<()> {
    let ext_handler = ExtensionsHandler {
        config: config.clone(),
    };
    let rt_handler = RuntimesHandler {
        config: config.clone(),
    };
    let hitl_handler = HitlHandler;
    let ra_handler = RootAuthorityHandler { config };

    let service = varlink::VarlinkService::new(
        "org.avocado",
        "avocadoctl",
        env!("CARGO_PKG_VERSION"),
        "https://avocado-linux.org",
        vec![
            Box::new(vl_ext::new(Box::new(ext_handler))),
            Box::new(vl_rt::new(Box::new(rt_handler))),
            Box::new(vl_hitl::new(Box::new(hitl_handler))),
            Box::new(vl_ra::new(Box::new(ra_handler))),
        ],
    );

    let listen_config = varlink::ListenConfig {
        idle_timeout: 0,
        ..Default::default()
    };

    varlink::listen(service, address, &listen_config)
}
