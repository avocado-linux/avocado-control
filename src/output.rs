//! Universal output management for avocadoctl
//!
//! This module provides a consistent interface for all output in the CLI,
//! handling verbosity levels and formatting consistently across all commands.

use std::io::Write;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

/// Output manager that handles verbosity and formatting consistently
pub struct OutputManager {
    verbose: bool,
}

impl OutputManager {
    /// Create a new output manager with the specified verbosity level
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    /// Print a colored prefix with message
    fn print_colored_prefix(&self, prefix: &str, color: Color, message: &str) {
        let color_choice = if std::env::var("NO_COLOR").is_ok() || std::env::var("AVOCADO_TEST_MODE").is_ok() {
            ColorChoice::Never
        } else {
            ColorChoice::Auto
        };

        let mut stdout = StandardStream::stdout(color_choice);
        let mut color_spec = ColorSpec::new();
        color_spec.set_fg(Some(color)).set_bold(true);

        if stdout.set_color(&color_spec).is_ok() && color_choice != ColorChoice::Never {
            let _ = write!(&mut stdout, "[{}]", prefix);
            let _ = stdout.reset();
            println!(" {}", message);
        } else {
            // Fallback for environments without color support
            println!("[{}] {}", prefix, message);
        }
    }

    /// Print a colored prefix with operation and message
    fn print_colored_prefix_with_op(&self, prefix: &str, color: Color, operation: &str, message: &str) {
        let color_choice = if std::env::var("NO_COLOR").is_ok() || std::env::var("AVOCADO_TEST_MODE").is_ok() {
            ColorChoice::Never
        } else {
            ColorChoice::Auto
        };

        let mut stdout = StandardStream::stdout(color_choice);
        let mut color_spec = ColorSpec::new();
        color_spec.set_fg(Some(color)).set_bold(true);

        if stdout.set_color(&color_spec).is_ok() && color_choice != ColorChoice::Never {
            let _ = write!(&mut stdout, "[{}]", prefix);
            let _ = stdout.reset();
            println!(" {}: {}", operation, message);
        } else {
            // Fallback for environments without color support
            println!("[{}] {}: {}", prefix, operation, message);
        }
    }

    /// Print a success message
    /// In non-verbose mode: shows brief success
    /// In verbose mode: shows detailed success with context
    pub fn success(&self, operation: &str, message: &str) {
        if self.verbose {
            self.print_colored_prefix_with_op("SUCCESS", Color::Green, operation, message);
        } else {
            self.print_colored_prefix("SUCCESS", Color::Green, message);
        }
    }

    /// Print an error message
    /// Always shows detailed error information for developers
    pub fn error(&self, operation: &str, message: &str) {
        let color_choice = if std::env::var("NO_COLOR").is_ok() || std::env::var("AVOCADO_TEST_MODE").is_ok() {
            ColorChoice::Never
        } else {
            ColorChoice::Auto
        };

        let mut stderr = StandardStream::stderr(color_choice);
        let mut color_spec = ColorSpec::new();
        color_spec.set_fg(Some(Color::Red)).set_bold(true);

        if stderr.set_color(&color_spec).is_ok() && color_choice != ColorChoice::Never {
            let _ = write!(&mut stderr, "[ERROR]");
            let _ = stderr.reset();
            eprintln!(" {}: {}", operation, message);
        } else {
            eprintln!("[ERROR] {}: {}", operation, message);
        }

        if !self.verbose {
            eprintln!("   Use --verbose for more details");
        }
    }

    /// Print an informational message
    /// In non-verbose mode: only shows important info
    /// In verbose mode: shows all info
    pub fn info(&self, operation: &str, message: &str) {
        if self.verbose {
            self.print_colored_prefix_with_op("INFO", Color::Blue, operation, message);
        }
    }

    /// Print detailed progress information (verbose only)
    pub fn progress(&self, message: &str) {
        if self.verbose {
            println!("   {message}");
        }
    }

    /// Print a step in a process (verbose only)
    pub fn step(&self, step: &str, description: &str) {
        if self.verbose {
            println!("   → {step}: {description}");
        }
    }

    /// Print raw output (like command results)
    pub fn raw(&self, content: &str) {
        if self.verbose {
            println!("{content}");
        }
    }

    /// Get the verbosity level
    pub fn is_verbose(&self) -> bool {
        self.verbose
    }

    /// Print a status header
    pub fn status_header(&self, title: &str) {
        if self.verbose {
            println!("\n{title}");
            println!("{}", "=".repeat(title.len()));
            println!();
        } else {
            println!("{title}");
        }
    }

    /// Print a brief status (always shown)
    pub fn status(&self, message: &str) {
        println!("{message}");
    }
}
