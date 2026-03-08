//! Universal output management for avocadoctl
//!
//! This module provides a consistent interface for all output in the CLI,
//! handling verbosity levels and formatting consistently across all commands.

use std::cell::RefCell;
use std::io::Write;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

/// Output manager that handles verbosity and formatting consistently
pub struct OutputManager {
    verbose: bool,
    json: bool,
    /// When set, messages are captured into this buffer instead of printed.
    /// Used by the service layer so the daemon can return log messages to
    /// varlink callers rather than printing to its own stdout.
    captured: Option<RefCell<Vec<String>>>,
}

impl OutputManager {
    /// Create a new output manager with the specified verbosity and format level
    pub fn new(verbose: bool, json: bool) -> Self {
        Self {
            verbose,
            json,
            captured: None,
        }
    }

    /// Create an output manager that captures messages instead of printing them.
    /// Use `take_messages()` to retrieve the collected messages.
    pub fn new_capturing() -> Self {
        Self {
            verbose: false,
            json: false,
            captured: Some(RefCell::new(Vec::new())),
        }
    }

    /// Whether this output manager is in capture mode.
    fn is_capturing(&self) -> bool {
        self.captured.is_some()
    }

    /// Push a message to the capture buffer.
    fn capture(&self, message: String) {
        if let Some(ref buf) = self.captured {
            buf.borrow_mut().push(message);
        }
    }

    /// Take all captured messages, leaving the buffer empty.
    pub fn take_messages(&self) -> Vec<String> {
        match self.captured {
            Some(ref buf) => std::mem::take(&mut *buf.borrow_mut()),
            None => Vec::new(),
        }
    }

    /// Whether output should be machine-readable JSON
    pub fn is_json(&self) -> bool {
        self.json
    }

    /// Determine the color choice for terminal output
    fn color_choice() -> ColorChoice {
        if std::env::var("NO_COLOR").is_ok() || std::env::var("AVOCADO_TEST_MODE").is_ok() {
            ColorChoice::Never
        } else {
            ColorChoice::Auto
        }
    }

    /// Print a colored prefix with message
    fn print_colored_prefix(&self, prefix: &str, color: Color, message: &str) {
        let color_choice = Self::color_choice();

        let mut stdout = StandardStream::stdout(color_choice);
        let mut color_spec = ColorSpec::new();
        color_spec.set_fg(Some(color)).set_bold(true);

        if stdout.set_color(&color_spec).is_ok() && color_choice != ColorChoice::Never {
            let _ = write!(&mut stdout, "[{prefix}]");
            let _ = stdout.reset();
            println!(" {message}");
        } else {
            // Fallback for environments without color support
            println!("[{prefix}] {message}");
        }
    }

    /// Print a colored prefix with operation and message
    fn print_colored_prefix_with_op(
        &self,
        prefix: &str,
        color: Color,
        operation: &str,
        message: &str,
    ) {
        let color_choice = Self::color_choice();

        let mut stdout = StandardStream::stdout(color_choice);
        let mut color_spec = ColorSpec::new();
        color_spec.set_fg(Some(color)).set_bold(true);

        if stdout.set_color(&color_spec).is_ok() && color_choice != ColorChoice::Never {
            let _ = write!(&mut stdout, "[{prefix}]");
            let _ = stdout.reset();
            println!(" {operation}: {message}");
        } else {
            // Fallback for environments without color support
            println!("[{prefix}] {operation}: {message}");
        }
    }

    /// Print a success message
    /// In non-verbose mode: shows brief success
    /// In verbose mode: shows detailed success with context
    /// Suppressed in JSON mode (structured output only)
    pub fn success(&self, operation: &str, message: &str) {
        if self.json {
            return;
        }
        if self.verbose {
            self.print_colored_prefix_with_op("SUCCESS", Color::Green, operation, message);
        } else {
            self.print_colored_prefix("SUCCESS", Color::Green, message);
        }
    }

    /// Print an error message
    /// Always shows detailed error information for developers
    pub fn error(&self, operation: &str, message: &str) {
        let color_choice = Self::color_choice();

        let mut stderr = StandardStream::stderr(color_choice);
        let mut color_spec = ColorSpec::new();
        color_spec.set_fg(Some(Color::Red)).set_bold(true);

        if stderr.set_color(&color_spec).is_ok() && color_choice != ColorChoice::Never {
            let _ = write!(&mut stderr, "[ERROR]");
            let _ = stderr.reset();
            eprintln!(" {operation}: {message}");
        } else {
            eprintln!("[ERROR] {operation}: {message}");
        }

        if !self.verbose {
            eprintln!("   Use --verbose for more details");
        }
    }

    /// Print an informational message
    /// Suppressed in JSON mode
    pub fn info(&self, operation: &str, message: &str) {
        if self.json {
            return;
        }
        if self.verbose {
            self.print_colored_prefix_with_op("INFO", Color::Blue, operation, message);
        }
    }

    /// Print detailed progress information (verbose only, suppressed in JSON mode)
    pub fn progress(&self, message: &str) {
        if self.json {
            return;
        }
        if self.verbose {
            println!("   {message}");
        }
    }

    /// Print a step in a process (verbose only, suppressed in JSON mode)
    pub fn step(&self, step: &str, description: &str) {
        if self.json {
            return;
        }
        if self.verbose {
            println!("   → {step}: {description}");
        }
    }

    /// Print raw output (like command results, suppressed in JSON mode)
    pub fn raw(&self, content: &str) {
        if self.json {
            return;
        }
        if self.verbose {
            println!("{content}");
        }
    }

    /// Get the verbosity level
    pub fn is_verbose(&self) -> bool {
        self.verbose
    }

    /// Print a status header (suppressed in JSON mode)
    pub fn status_header(&self, title: &str) {
        if self.json {
            return;
        }
        if self.verbose {
            println!("\n{title}");
            println!("{}", "=".repeat(title.len()));
            println!();
        } else {
            println!("{title}");
        }
    }

    /// Print a brief status (suppressed in JSON mode)
    pub fn status(&self, message: &str) {
        if self.json {
            return;
        }
        println!("{message}");
    }

    /// Log an informational message.
    ///
    /// In normal mode: prints to stdout with color (always, regardless of verbosity).
    /// In capture mode: captures to the message buffer for returning via varlink.
    pub fn log_info(&self, message: &str) {
        if self.is_capturing() {
            self.capture(format!("[INFO] {message}"));
        } else if !self.json {
            self.print_colored_prefix("INFO", Color::Blue, message);
        }
    }

    /// Log a success message.
    ///
    /// In normal mode: prints to stdout with color (always, regardless of verbosity).
    /// In capture mode: captures to the message buffer for returning via varlink.
    pub fn log_success(&self, message: &str) {
        if self.is_capturing() {
            self.capture(format!("[SUCCESS] {message}"));
        } else if !self.json {
            self.print_colored_prefix("SUCCESS", Color::Green, message);
        }
    }
}
