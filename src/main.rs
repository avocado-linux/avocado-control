mod commands;
mod config;
pub mod manifest;
mod output;
pub mod service;
pub mod staging;
pub mod update;
mod varlink;
mod varlink_server;

use clap::{Arg, Command};
use commands::{ext, hitl, root_authority, runtime};
use config::Config;
use output::OutputManager;

fn main() {
    let app = Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file")
                .global(true),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose output")
                .action(clap::ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FORMAT")
                .help("Output format: table (default) or json")
                .global(true)
                .default_value("table"),
        )
        .subcommand(ext::create_command())
        .subcommand(hitl::create_command())
        .subcommand(root_authority::create_command())
        .subcommand(runtime::create_command())
        .subcommand(
            Command::new("status").about("Show overall system status including extensions"),
        )
        // Top-level aliases for common ext commands
        .subcommand(
            Command::new("merge")
                .about("Merge extensions using systemd-sysext and systemd-confext (alias for 'ext merge')"),
        )
        .subcommand(
            Command::new("unmerge")
                .about("Unmerge extensions using systemd-sysext and systemd-confext (alias for 'ext unmerge')")
                .arg(
                    Arg::new("unmount")
                        .long("unmount")
                        .help("Also unmount all persistent loops for .raw extensions")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("refresh")
                .about("Unmerge and then merge extensions (alias for 'ext refresh')"),
        )
        .subcommand(
            Command::new("enable")
                .about("Enable extensions for a specific runtime version")
                .arg(
                    Arg::new("os_release")
                        .long("os-release")
                        .value_name("VERSION")
                        .help("OS release version (defaults to current os-release VERSION_ID)"),
                )
                .arg(
                    Arg::new("extensions")
                        .help("Extension names to enable")
                        .required(true)
                        .num_args(1..)
                        .value_name("EXTENSION"),
                ),
        )
        .subcommand(
            Command::new("disable")
                .about("Disable extensions for a specific runtime version")
                .arg(
                    Arg::new("os_release")
                        .long("os-release")
                        .value_name("VERSION")
                        .help("OS release version (defaults to current os-release VERSION_ID)"),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Disable all extensions")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("extensions")
                        .help("Extension names to disable")
                        .required_unless_present("all")
                        .num_args(1..)
                        .value_name("EXTENSION"),
                ),
        )
        .subcommand(
            Command::new("serve")
                .about("Start the Varlink IPC server")
                .arg(
                    Arg::new("address")
                        .long("address")
                        .value_name("ADDRESS")
                        .help("Listen address (e.g. unix:/run/avocado/avocadoctl.sock)")
                        .default_value("unix:/run/avocado/avocadoctl.sock"),
                ),
        );

    let matches = app.get_matches();

    // Initialize output manager with global verbose and format settings
    let verbose = matches.get_flag("verbose");
    let json_output = matches
        .get_one::<String>("output")
        .map(|s| s == "json")
        .unwrap_or(false);
    let output = OutputManager::new(verbose, json_output);

    // Load configuration
    let config_path = matches.get_one::<String>("config").map(|s| s.as_str());
    let config = match Config::load_with_override(config_path) {
        Ok(config) => config,
        Err(e) => {
            output.error(
                "Configuration Error",
                &format!("Failed to load configuration: {e}"),
            );
            std::process::exit(1);
        }
    };

    match matches.subcommand() {
        Some(("ext", ext_matches)) => {
            ext::handle_command(ext_matches, &config, &output);
        }
        Some(("hitl", hitl_matches)) => {
            hitl::handle_command(hitl_matches, &output);
        }
        Some(("root-authority", _)) => {
            root_authority::handle_command(&config, &output);
        }
        Some(("runtime", runtime_matches)) => {
            runtime::handle_command(runtime_matches, &config, &output);
        }
        Some(("serve", serve_matches)) => {
            let address = serve_matches
                .get_one::<String>("address")
                .expect("address has a default value");
            if let Err(e) = varlink_server::run_server(address, config) {
                output.error("Server Error", &format!("Varlink server failed: {e}"));
                std::process::exit(1);
            }
        }
        Some(("status", _)) => {
            show_system_status(&config, &output);
        }
        // Top-level command aliases
        Some(("merge", _)) => {
            ext::merge_extensions_direct(&output);
            json_ok(&output);
        }
        Some(("unmerge", unmerge_matches)) => {
            let unmount = unmerge_matches.get_flag("unmount");
            ext::unmerge_extensions_direct(unmount, &output);
            json_ok(&output);
        }
        Some(("refresh", _)) => {
            ext::refresh_extensions_direct(&output);
            json_ok(&output);
        }
        Some(("enable", enable_matches)) => {
            let os_release = enable_matches
                .get_one::<String>("os_release")
                .map(|s| s.as_str());
            let extensions: Vec<&str> = enable_matches
                .get_many::<String>("extensions")
                .unwrap()
                .map(|s| s.as_str())
                .collect();
            ext::enable_extensions(os_release, &extensions, &config, &output);
            json_ok(&output);
        }
        Some(("disable", disable_matches)) => {
            let os_release = disable_matches
                .get_one::<String>("os_release")
                .map(|s| s.as_str());
            let all = disable_matches.get_flag("all");
            let extensions: Option<Vec<&str>> = disable_matches
                .get_many::<String>("extensions")
                .map(|values| values.map(|s| s.as_str()).collect());
            ext::disable_extensions(os_release, extensions.as_deref(), all, &config, &output);
            json_ok(&output);
        }
        _ => {
            println!(
                "{} - {}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_DESCRIPTION")
            );
            println!("Use --help for more information or --version for version details");
        }
    }
}

/// Show overall system status including extensions
fn show_system_status(config: &Config, output: &OutputManager) {
    output.info("System Status", "Checking overall system status");
    ext::status_extensions(config, output);
}

/// Emit a JSON success result when in JSON mode (no-op otherwise).
/// Action commands that exit(1) on failure never reach this,
/// so it only runs on success.
fn json_ok(output: &OutputManager) {
    if output.is_json() {
        println!("{{\"status\":\"ok\"}}");
    }
}
