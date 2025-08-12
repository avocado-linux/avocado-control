use clap::Command;

fn main() {
    let app = Command::new("avocadoctl")
        .version("0.1.0")
        .author("Your Name <your.email@example.com>")
        .about("Avocado Linux control CLI tool");

    let _matches = app.get_matches();

    // If we reach here, no version flag was used
    println!("avocadoctl - Avocado Linux control CLI tool");
    println!("Use --help for more information or --version for version details");
}
