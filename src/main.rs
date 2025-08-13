mod commands;
mod config;

use clap::{Arg, Command};
use commands::{ext, hitl};
use config::Config;

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
        .subcommand(ext::create_command())
        .subcommand(hitl::create_command())
        .subcommand(
            Command::new("status").about("Show overall system status including extensions"),
        );

    let matches = app.get_matches();

    // Load configuration
    let config_path = matches.get_one::<String>("config").map(|s| s.as_str());
    let config = match Config::load_with_override(config_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error loading configuration: {e}");
            std::process::exit(1);
        }
    };

    match matches.subcommand() {
        Some(("ext", ext_matches)) => {
            ext::handle_command(ext_matches, &config);
        }
        Some(("hitl", hitl_matches)) => {
            hitl::handle_command(hitl_matches);
        }
        Some(("status", _)) => {
            show_system_status();
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
fn show_system_status() {
    println!("Avocado System Status");
    println!("====================");
    println!();

    // For now, just show extension status
    // In the future, this could include other system components
    ext::status_extensions();
}
