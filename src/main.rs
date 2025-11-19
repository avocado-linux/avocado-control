mod commands;
mod config;
mod output;

use clap::{Arg, Command};
use commands::{ext, hitl};
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
        .subcommand(ext::create_command())
        .subcommand(hitl::create_command())
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
                    Arg::new("runtime")
                        .long("runtime")
                        .value_name("VERSION")
                        .help("Runtime version (defaults to current os-release VERSION_ID)"),
                )
                .arg(
                    Arg::new("extensions")
                        .help("Extension names to enable")
                        .required(true)
                        .num_args(1..)
                        .value_name("EXTENSION"),
                ),
        );

    let matches = app.get_matches();

    // Initialize output manager with global verbose setting
    let verbose = matches.get_flag("verbose");
    let output = OutputManager::new(verbose);

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
        Some(("status", _)) => {
            show_system_status(&output);
        }
        // Top-level command aliases
        Some(("merge", _)) => {
            ext::merge_extensions_direct(&output);
        }
        Some(("unmerge", unmerge_matches)) => {
            let unmount = unmerge_matches.get_flag("unmount");
            ext::unmerge_extensions_direct(unmount, &output);
        }
        Some(("refresh", _)) => {
            ext::refresh_extensions_direct(&output);
        }
        Some(("enable", enable_matches)) => {
            let runtime = enable_matches.get_one::<String>("runtime").map(|s| s.as_str());
            let extensions: Vec<&str> = enable_matches
                .get_many::<String>("extensions")
                .unwrap()
                .map(|s| s.as_str())
                .collect();
            ext::enable_extensions(runtime, &extensions, &config, &output);
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
fn show_system_status(output: &OutputManager) {
    output.info("System Status", "Checking overall system status");
    ext::status_extensions(output);
}
