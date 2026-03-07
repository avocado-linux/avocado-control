use crate::output::OutputManager;
use crate::varlink::{
    org_avocado_Extensions as vl_ext, org_avocado_Hitl as vl_hitl,
    org_avocado_RootAuthority as vl_ra, org_avocado_Runtimes as vl_rt,
};
use std::sync::{Arc, RwLock};
use varlink::Connection;

pub use vl_ext::VarlinkClientInterface as ExtClientInterface;
pub use vl_hitl::VarlinkClientInterface as HitlClientInterface;
pub use vl_ra::VarlinkClientInterface as RaClientInterface;
pub use vl_rt::VarlinkClientInterface as RtClientInterface;

/// Connect to the varlink daemon socket.
/// Prints an error and exits with code 1 if the daemon is not reachable.
pub fn connect_or_exit(address: &str, output: &OutputManager) -> Arc<RwLock<Connection>> {
    match varlink::Connection::with_address(address) {
        Ok(conn) => conn,
        Err(e) => {
            output.error(
                "Daemon Not Running",
                &format!(
                    "Cannot connect to avocadoctl daemon at {address}: {e}\n   \
                     Start it with: systemctl start avocadoctl"
                ),
            );
            std::process::exit(1);
        }
    }
}

/// Print an RPC error and exit with code 1.
pub fn exit_with_rpc_error(err: impl std::fmt::Display, output: &OutputManager) -> ! {
    output.error("RPC Error", &err.to_string());
    std::process::exit(1);
}

// ── Extension output helpers ─────────────────────────────────────────────────

pub fn print_extensions(extensions: &[vl_ext::Extension], output: &OutputManager) {
    if output.is_json() {
        match serde_json::to_string(extensions) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                output.error("Output", &format!("JSON serialization failed: {e}"));
                std::process::exit(1);
            }
        }
        return;
    }

    if extensions.is_empty() {
        println!("No extensions found.");
        return;
    }

    let name_width = extensions
        .iter()
        .map(|e| {
            e.name.len()
                + e.version
                    .as_ref()
                    .map(|v| v.len() + 1)
                    .unwrap_or(0)
        })
        .max()
        .unwrap_or(9)
        .max(9);

    println!(
        "{:<nw$} {:<12} {}",
        "Extension",
        "Type",
        "Path",
        nw = name_width
    );
    println!("{}", "=".repeat(name_width + 1 + 12 + 1 + 20));

    for ext in extensions {
        let versioned_name = match &ext.version {
            Some(v) => format!("{}-{}", ext.name, v),
            None => ext.name.clone(),
        };

        let mut types = Vec::new();
        if ext.isSysext {
            types.push("sys");
        }
        if ext.isConfext {
            types.push("conf");
        }
        let type_str = if types.is_empty() {
            "?".to_string()
        } else {
            types.join("+")
        };

        println!(
            "{:<nw$} {:<12} {}",
            versioned_name,
            type_str,
            ext.path,
            nw = name_width
        );
    }

    println!();
    println!("Total: {} extension(s)", extensions.len());
}

pub fn print_extension_status(extensions: &[vl_ext::ExtensionStatus], output: &OutputManager) {
    if output.is_json() {
        match serde_json::to_string(extensions) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                output.error("Output", &format!("JSON serialization failed: {e}"));
                std::process::exit(1);
            }
        }
        return;
    }

    if extensions.is_empty() {
        println!("No extensions currently merged.");
        return;
    }

    let name_width = extensions
        .iter()
        .map(|e| {
            e.name.len()
                + e.version
                    .as_ref()
                    .map(|v| v.len() + 1)
                    .unwrap_or(0)
        })
        .max()
        .unwrap_or(9)
        .max(9);

    println!(
        "{:<nw$} {:<12} {:<8} {}",
        "Extension",
        "Type",
        "Merged",
        "Origin",
        nw = name_width
    );
    println!("{}", "=".repeat(name_width + 1 + 12 + 1 + 8 + 1 + 20));

    for ext in extensions {
        let versioned_name = match &ext.version {
            Some(v) => format!("{}-{}", ext.name, v),
            None => ext.name.clone(),
        };

        let mut types = Vec::new();
        if ext.isSysext {
            types.push("sys");
        }
        if ext.isConfext {
            types.push("conf");
        }
        let type_str = if types.is_empty() {
            "?".to_string()
        } else {
            types.join("+")
        };

        let merged_str = if ext.isMerged { "yes" } else { "no" };
        let origin = ext.origin.as_deref().unwrap_or("-");

        println!(
            "{:<nw$} {:<12} {:<8} {}",
            versioned_name,
            type_str,
            merged_str,
            origin,
            nw = name_width
        );
    }

    println!();
    let merged_count = extensions.iter().filter(|e| e.isMerged).count();
    println!("Total: {} extension(s), {} merged", extensions.len(), merged_count);
}

// ── Runtime output helpers ────────────────────────────────────────────────────

pub fn print_runtimes(runtimes: &[vl_rt::Runtime], output: &OutputManager) {
    if output.is_json() {
        match serde_json::to_string(runtimes) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                output.error("Output", &format!("JSON serialization failed: {e}"));
                std::process::exit(1);
            }
        }
        return;
    }

    if runtimes.is_empty() {
        println!("No runtimes found.");
        return;
    }

    let id_width = runtimes
        .iter()
        .map(|r| r.id.len().min(12))
        .max()
        .unwrap_or(8)
        .max(8);

    println!(
        "{:<iw$} {:<20} {:<12} {}",
        "ID",
        "Runtime",
        "Active",
        "Built At",
        iw = id_width
    );
    println!("{}", "=".repeat(id_width + 1 + 20 + 1 + 12 + 1 + 20));

    for rt in runtimes {
        let short_id = &rt.id[..rt.id.len().min(12)];
        let runtime_label = format!("{} {}", rt.runtime.name, rt.runtime.version);
        let active_str = if rt.active { "* active" } else { "" };

        println!(
            "{:<iw$} {:<20} {:<12} {}",
            short_id,
            runtime_label,
            active_str,
            rt.builtAt,
            iw = id_width
        );
    }

    println!();
    println!("Total: {} runtime(s)", runtimes.len());
}

pub fn print_runtime_detail(rt: &vl_rt::Runtime, output: &OutputManager) {
    if output.is_json() {
        match serde_json::to_string(rt) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                output.error("Output", &format!("JSON serialization failed: {e}"));
                std::process::exit(1);
            }
        }
        return;
    }

    println!();
    println!("  Runtime: {} {}", rt.runtime.name, rt.runtime.version);
    println!("  ID:      {}", rt.id);
    println!("  Built:   {}", rt.builtAt);
    println!("  Active:  {}", if rt.active { "yes" } else { "no" });

    if !rt.extensions.is_empty() {
        println!();
        println!("  Extensions:");
        for ext in &rt.extensions {
            let img = ext.imageId.as_deref().unwrap_or("-");
            println!("    {} {} (image: {})", ext.name, ext.version, img);
        }
    }
    println!();
}

// ── Root authority output helper ──────────────────────────────────────────────

pub fn print_root_authority(info: &Option<vl_ra::RootAuthorityInfo>, output: &OutputManager) {
    if output.is_json() {
        match serde_json::to_string(info) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                output.error("Output", &format!("JSON serialization failed: {e}"));
                std::process::exit(1);
            }
        }
        return;
    }

    match info {
        None => {
            output.info(
                "Root Authority",
                "No root authority configured. Build and provision a runtime with avocado build to enable verified updates.",
            );
        }
        Some(ra) => {
            println!();
            println!("  Root authority:");
            println!();
            println!("    Version:  {}", ra.version);
            println!("    Expires:  {}", ra.expires);
            println!();
            println!("    Trusted signing keys:");
            println!();
            println!("      {:<18} {:<12} ROLES", "KEY ID", "TYPE");
            for key in &ra.keys {
                let short_id = &key.keyId[..key.keyId.len().min(16)];
                let roles_str = key.roles.join(", ");
                println!("      {short_id:<18} {:<12} {roles_str}", key.keyType);
            }
            println!();
        }
    }
}
