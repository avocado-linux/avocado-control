//! Universal output management for avocadoctl
//!
//! This module provides a consistent interface for all output in the CLI,
//! handling verbosity levels and formatting consistently across all commands.

/// Output manager that handles verbosity and formatting consistently
pub struct OutputManager {
    verbose: bool,
}

impl OutputManager {
    /// Create a new output manager with the specified verbosity level
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    /// Print a success message
    /// In non-verbose mode: shows brief success
    /// In verbose mode: shows detailed success with context
    pub fn success(&self, operation: &str, message: &str) {
        if self.verbose {
            println!("✅ {operation}: {message}");
        } else {
            println!("✅ {message}");
        }
    }

    /// Print an error message
    /// Always shows detailed error information for developers
    pub fn error(&self, operation: &str, message: &str) {
        eprintln!("❌ {operation}: {message}");
        if !self.verbose {
            eprintln!("   Use --verbose for more details");
        }
    }

    /// Print an informational message
    /// In non-verbose mode: only shows important info
    /// In verbose mode: shows all info
    pub fn info(&self, operation: &str, message: &str) {
        if self.verbose {
            println!("ℹ️  {operation}: {message}");
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
