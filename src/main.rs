mod commands;
mod config;

use clap::{Arg, Command};
use commands::ext;
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
        .subcommand(ext::create_command());

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
