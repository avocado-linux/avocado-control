mod commands;
mod config;
pub mod manifest;
mod output;
pub mod service;
pub mod staging;
pub mod update;
mod varlink;
mod varlink_client;
mod varlink_server;

use clap::{Arg, Command};
use commands::{ext, hitl, root_authority, runtime};
use config::Config;
use output::OutputManager;
use varlink::org_avocado_Extensions as vl_ext;
use varlink::org_avocado_Hitl as vl_hitl;
use varlink::org_avocado_RootAuthority as vl_ra;
use varlink::org_avocado_Runtimes as vl_rt;
use varlink_client::{
    ExtClientInterface, HitlClientInterface, RaClientInterface, RtClientInterface,
};

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
        .arg(
            Arg::new("socket")
                .long("socket")
                .value_name("ADDRESS")
                .help("Varlink daemon socket address (overrides config)")
                .global(true),
        )
        .subcommand(commands::ext::create_command())
        .subcommand(commands::hitl::create_command())
        .subcommand(commands::root_authority::create_command())
        .subcommand(commands::runtime::create_command())
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

    // Resolve socket address: CLI flag > config > default
    let socket_address = matches
        .get_one::<String>("socket")
        .cloned()
        .unwrap_or_else(|| config.socket_address().to_string());

    // In test mode, skip the varlink daemon and call service functions directly.
    // This allows existing integration tests (which use AVOCADO_TEST_MODE=1 with mock
    // executables) to keep running without needing a live daemon.
    if std::env::var("AVOCADO_TEST_MODE").is_ok() {
        handle_direct(&matches, &config, &output);
        return;
    }

    match matches.subcommand() {
        // ── ext subcommands ──────────────────────────────────────────────────
        Some(("ext", ext_matches)) => {
            let conn = varlink_client::connect_or_exit(&socket_address, &output);
            match ext_matches.subcommand() {
                Some(("list", _)) => {
                    let mut client = vl_ext::VarlinkClient::new(conn);
                    match client.list().call() {
                        Ok(reply) => varlink_client::print_extensions(&reply.extensions, &output),
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                }
                Some(("merge", _)) => {
                    let mut client = vl_ext::VarlinkClient::new(conn);
                    match client.merge().call() {
                        Ok(reply) => {
                            varlink_client::print_log(&reply.log);
                            output.success("Merge", "Extensions merged successfully");
                        }
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                    json_ok(&output);
                }
                Some(("unmerge", unmerge_matches)) => {
                    let unmount = unmerge_matches.get_flag("unmount");
                    let mut client = vl_ext::VarlinkClient::new(conn);
                    match client.unmerge(Some(unmount)).call() {
                        Ok(reply) => {
                            varlink_client::print_log(&reply.log);
                            output.success("Unmerge", "Extensions unmerged successfully");
                        }
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                    json_ok(&output);
                }
                Some(("refresh", _)) => {
                    let mut client = vl_ext::VarlinkClient::new(conn);
                    match client.refresh().call() {
                        Ok(reply) => {
                            varlink_client::print_log(&reply.log);
                            output.success("Refresh", "Extensions refreshed successfully");
                        }
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                    json_ok(&output);
                }
                Some(("status", _)) => {
                    let mut client = vl_ext::VarlinkClient::new(conn);
                    match client.status().call() {
                        Ok(reply) => {
                            varlink_client::print_extension_status(&reply.extensions, &output)
                        }
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                }
                _ => {
                    println!("Use 'avocadoctl ext --help' for available extension commands");
                }
            }
        }

        // ── hitl subcommands ─────────────────────────────────────────────────
        Some(("hitl", hitl_matches)) => {
            let conn = varlink_client::connect_or_exit(&socket_address, &output);
            match hitl_matches.subcommand() {
                Some(("mount", mount_matches)) => {
                    let server_ip = mount_matches
                        .get_one::<String>("server-ip")
                        .expect("server-ip is required")
                        .clone();
                    let server_port = mount_matches.get_one::<String>("server-port").cloned();
                    let extensions: Vec<String> = mount_matches
                        .get_many::<String>("extension")
                        .expect("at least one extension is required")
                        .cloned()
                        .collect();
                    let mut client = vl_hitl::VarlinkClient::new(conn);
                    match client.mount(server_ip, server_port, extensions).call() {
                        Ok(_) => output.success("HITL Mount", "Extensions mounted successfully"),
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                    json_ok(&output);
                }
                Some(("unmount", unmount_matches)) => {
                    let extensions: Vec<String> = unmount_matches
                        .get_many::<String>("extension")
                        .expect("at least one extension is required")
                        .cloned()
                        .collect();
                    let mut client = vl_hitl::VarlinkClient::new(conn);
                    match client.unmount(extensions).call() {
                        Ok(_) => {
                            output.success("HITL Unmount", "Extensions unmounted successfully")
                        }
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                    json_ok(&output);
                }
                _ => {
                    println!("Use 'avocadoctl hitl --help' for available HITL commands");
                }
            }
        }

        // ── root-authority ───────────────────────────────────────────────────
        Some(("root-authority", _)) => {
            let conn = varlink_client::connect_or_exit(&socket_address, &output);
            let mut client = vl_ra::VarlinkClient::new(conn);
            match client.show().call() {
                Ok(reply) => varlink_client::print_root_authority(&reply.authority, &output),
                Err(e) => varlink_client::exit_with_rpc_error(e, &output),
            }
        }

        // ── runtime subcommands ──────────────────────────────────────────────
        Some(("runtime", runtime_matches)) => {
            let conn = varlink_client::connect_or_exit(&socket_address, &output);
            match runtime_matches.subcommand() {
                Some(("list", _)) => {
                    let mut client = vl_rt::VarlinkClient::new(conn);
                    match client.list().call() {
                        Ok(reply) => varlink_client::print_runtimes(&reply.runtimes, &output),
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                }
                Some(("add", add_matches)) => {
                    if let Some(url) = add_matches.get_one::<String>("url") {
                        let auth_token = std::env::var("AVOCADO_TUF_AUTH_TOKEN").ok();
                        let mut client = vl_rt::VarlinkClient::new(conn);
                        match client.add_from_url(url.clone(), auth_token).call() {
                            Ok(reply) => {
                                varlink_client::print_log(&reply.log);
                                output.success("Runtime Add", "Runtime added successfully");
                            }
                            Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                        }
                    } else if let Some(manifest) = add_matches.get_one::<String>("manifest") {
                        let mut client = vl_rt::VarlinkClient::new(conn);
                        match client.add_from_manifest(manifest.clone()).call() {
                            Ok(reply) => {
                                varlink_client::print_log(&reply.log);
                                output.success("Runtime Add", "Runtime added successfully");
                            }
                            Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                        }
                    }
                    json_ok(&output);
                }
                Some(("remove", remove_matches)) => {
                    let id = remove_matches
                        .get_one::<String>("id")
                        .expect("id is required")
                        .clone();
                    let mut client = vl_rt::VarlinkClient::new(conn);
                    match client.remove(id).call() {
                        Ok(_) => output.success("Runtime Remove", "Runtime removed successfully"),
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                    json_ok(&output);
                }
                Some(("activate", activate_matches)) => {
                    let id = activate_matches
                        .get_one::<String>("id")
                        .expect("id is required")
                        .clone();
                    let mut client = vl_rt::VarlinkClient::new(conn);
                    match client.activate(id).call() {
                        Ok(reply) => {
                            varlink_client::print_log(&reply.log);
                            output.success("Runtime Activate", "Runtime activated successfully");
                        }
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                    json_ok(&output);
                }
                Some(("inspect", inspect_matches)) => {
                    let id = inspect_matches
                        .get_one::<String>("id")
                        .expect("id is required")
                        .clone();
                    let mut client = vl_rt::VarlinkClient::new(conn);
                    match client.inspect(id).call() {
                        Ok(reply) => varlink_client::print_runtime_detail(&reply.runtime, &output),
                        Err(e) => varlink_client::exit_with_rpc_error(e, &output),
                    }
                }
                _ => {
                    println!("Use 'runtime list' to see available runtimes.");
                    println!("Run 'avocadoctl runtime --help' for more information.");
                }
            }
        }

        // ── serve (starts the daemon — direct, no varlink client) ────────────
        Some(("serve", serve_matches)) => {
            let address = serve_matches
                .get_one::<String>("address")
                .expect("address has a default value");
            if let Err(e) = varlink_server::run_server(address, config) {
                output.error("Server Error", &format!("Varlink server failed: {e}"));
                std::process::exit(1);
            }
        }

        // ── status (top-level) ───────────────────────────────────────────────
        Some(("status", _)) => {
            let conn = varlink_client::connect_or_exit(&socket_address, &output);
            let mut client = vl_ext::VarlinkClient::new(conn);
            match client.status().call() {
                Ok(reply) => {
                    output.status_header("System Status");
                    varlink_client::print_extension_status(&reply.extensions, &output);
                }
                Err(e) => varlink_client::exit_with_rpc_error(e, &output),
            }
        }

        // ── Top-level aliases ────────────────────────────────────────────────
        Some(("merge", _)) => {
            let conn = varlink_client::connect_or_exit(&socket_address, &output);
            let mut client = vl_ext::VarlinkClient::new(conn);
            match client.merge().call() {
                Ok(reply) => {
                    varlink_client::print_log(&reply.log);
                    output.success("Merge", "Extensions merged successfully");
                }
                Err(e) => varlink_client::exit_with_rpc_error(e, &output),
            }
            json_ok(&output);
        }
        Some(("unmerge", unmerge_matches)) => {
            let unmount = unmerge_matches.get_flag("unmount");
            let conn = varlink_client::connect_or_exit(&socket_address, &output);
            let mut client = vl_ext::VarlinkClient::new(conn);
            match client.unmerge(Some(unmount)).call() {
                Ok(reply) => {
                    varlink_client::print_log(&reply.log);
                    output.success("Unmerge", "Extensions unmerged successfully");
                }
                Err(e) => varlink_client::exit_with_rpc_error(e, &output),
            }
            json_ok(&output);
        }
        Some(("refresh", _)) => {
            let conn = varlink_client::connect_or_exit(&socket_address, &output);
            let mut client = vl_ext::VarlinkClient::new(conn);
            match client.refresh().call() {
                Ok(reply) => {
                    varlink_client::print_log(&reply.log);
                    output.success("Refresh", "Extensions refreshed successfully");
                }
                Err(e) => varlink_client::exit_with_rpc_error(e, &output),
            }
            json_ok(&output);
        }
        Some(("enable", enable_matches)) => {
            let os_release = enable_matches.get_one::<String>("os_release").cloned();
            let extensions: Vec<String> = enable_matches
                .get_many::<String>("extensions")
                .unwrap()
                .cloned()
                .collect();
            let conn = varlink_client::connect_or_exit(&socket_address, &output);
            let mut client = vl_ext::VarlinkClient::new(conn);
            match client.enable(extensions, os_release).call() {
                Ok(reply) => {
                    if !output.is_json() {
                        output.success(
                            "Enable",
                            &format!(
                                "{} extension(s) enabled, {} failed",
                                reply.enabled, reply.failed
                            ),
                        );
                    }
                }
                Err(e) => varlink_client::exit_with_rpc_error(e, &output),
            }
            json_ok(&output);
        }
        Some(("disable", disable_matches)) => {
            let os_release = disable_matches.get_one::<String>("os_release").cloned();
            let all = disable_matches.get_flag("all");
            let extensions: Option<Vec<String>> = disable_matches
                .get_many::<String>("extensions")
                .map(|values| values.cloned().collect());
            let conn = varlink_client::connect_or_exit(&socket_address, &output);
            let mut client = vl_ext::VarlinkClient::new(conn);
            match client.disable(extensions, Some(all), os_release).call() {
                Ok(reply) => {
                    if !output.is_json() {
                        output.success(
                            "Disable",
                            &format!(
                                "{} extension(s) disabled, {} failed",
                                reply.disabled, reply.failed
                            ),
                        );
                    }
                }
                Err(e) => varlink_client::exit_with_rpc_error(e, &output),
            }
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

/// Direct dispatch used when AVOCADO_TEST_MODE is set.
/// Calls service functions directly, bypassing the varlink daemon.
/// This keeps existing integration tests (with mock executables) working
/// without needing a live daemon process.
fn handle_direct(matches: &clap::ArgMatches, config: &Config, output: &OutputManager) {
    match matches.subcommand() {
        Some(("ext", ext_matches)) => {
            ext::handle_command(ext_matches, config, output);
        }
        Some(("hitl", hitl_matches)) => {
            hitl::handle_command(hitl_matches, output);
        }
        Some(("root-authority", _)) => {
            root_authority::handle_command(config, output);
        }
        Some(("runtime", runtime_matches)) => {
            runtime::handle_command(runtime_matches, config, output);
        }
        Some(("serve", serve_matches)) => {
            let address = serve_matches
                .get_one::<String>("address")
                .expect("address has a default value");
            if let Err(e) = varlink_server::run_server(address, config.clone()) {
                output.error("Server Error", &format!("Varlink server failed: {e}"));
                std::process::exit(1);
            }
        }
        Some(("status", _)) => {
            output.status_header("System Status");
            ext::status_extensions(config, output);
        }
        Some(("merge", _)) => {
            ext::merge_extensions_direct(output);
            json_ok(output);
        }
        Some(("unmerge", unmerge_matches)) => {
            let unmount = unmerge_matches.get_flag("unmount");
            ext::unmerge_extensions_direct(unmount, output);
            json_ok(output);
        }
        Some(("refresh", _)) => {
            ext::refresh_extensions_direct(output);
            json_ok(output);
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
            ext::enable_extensions(os_release, &extensions, config, output);
            json_ok(output);
        }
        Some(("disable", disable_matches)) => {
            let os_release = disable_matches
                .get_one::<String>("os_release")
                .map(|s| s.as_str());
            let all = disable_matches.get_flag("all");
            let extensions: Option<Vec<&str>> = disable_matches
                .get_many::<String>("extensions")
                .map(|values| values.map(|s| s.as_str()).collect());
            ext::disable_extensions(os_release, extensions.as_deref(), all, config, output);
            json_ok(output);
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

/// Emit a JSON success result when in JSON mode (no-op otherwise).
/// Action commands that exit(1) on failure never reach this,
/// so it only runs on success.
fn json_ok(output: &OutputManager) {
    if output.is_json() {
        println!("{{\"status\":\"ok\"}}");
    }
}
