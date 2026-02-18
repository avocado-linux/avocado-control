use crate::config::Config;
use crate::manifest::RuntimeManifest;
use crate::output::OutputManager;
use clap::{ArgMatches, Command};
use std::path::Path;

pub fn create_command() -> Command {
    Command::new("runtime")
        .about("Manage runtimes")
        .subcommand(Command::new("list").about("List available runtimes"))
}

pub fn handle_command(matches: &ArgMatches, config: &Config, output: &OutputManager) {
    match matches.subcommand() {
        Some(("list", _)) => {
            list_runtimes(config, output);
        }
        _ => {
            println!("Use 'runtime list' to see available runtimes.");
            println!("Run 'avocadoctl runtime --help' for more information.");
        }
    }
}

fn list_runtimes(config: &Config, output: &OutputManager) {
    let base_dir = config.get_avocado_base_dir();
    let base_path = Path::new(&base_dir);

    let runtimes = RuntimeManifest::list_all(base_path);

    if runtimes.is_empty() {
        output.info(
            "Runtime List",
            "No runtimes found. Build and provision a runtime first.",
        );
        return;
    }

    println!();
    println!(
        "  {:<16} {:<12} {:<10} {:<24} STATUS",
        "NAME", "VERSION", "BUILD ID", "BUILT AT"
    );

    for (manifest, is_active) in &runtimes {
        let short_id = if manifest.id.len() >= 8 {
            &manifest.id[..8]
        } else {
            &manifest.id
        };

        let built_at_display = manifest.built_at.replace('T', " ").replace('Z', "");

        let status = if *is_active { "active" } else { "" };

        println!(
            "  {:<16} {:<12} {:<10} {:<24} {}",
            manifest.runtime.name, manifest.runtime.version, short_id, built_at_display, status
        );
    }

    println!();

    if output.is_verbose() {
        println!("  Full build IDs:");
        for (manifest, is_active) in &runtimes {
            let marker = if *is_active { " (active)" } else { "" };
            println!(
                "    {} {}{marker}",
                manifest.id, manifest.runtime.name,
            );
        }
        println!();
    }
}
