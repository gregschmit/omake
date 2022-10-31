use std::error::Error;
use std::fmt;

use super::Context;

/// Principal formatter for log messages.
///
/// Since existing tooling around `make` most likely targets GNU `make`, we should probably mimic
/// that implementation's logging conventions. I imagine existing tools parse STDERR to display
/// warnings and errors in the makefile on the associated lines.
fn format_log<S: Into<String>>(msg: S, level: &str, context: Option<&Context>) -> String {
    // Format context.
    let context_display = match context {
        None => "make:".to_string(),
        Some(context) => match &context.path {
            None => "make:".to_string(),
            Some(path) => {
                if context.line_number == 0 {
                    format!("{}:{}:", path.display(), context.line_number)
                } else {
                    format!("{}:", path.display())
                }
            }
        },
    };

    // Format log level.
    let level_display = format!("{:5}", level.to_string());

    // Print the log message.
    format!("{context_display} {level_display} | {}", msg.into())
}

/// Helper to format info.
fn format_info<S: Into<String>>(msg: S, context: Option<&Context>) -> String {
    format_log(msg, "INFO", context)
}

/// Helper to format warnings.
fn format_warn<S: Into<String>>(msg: S, context: Option<&Context>) -> String {
    format_log(msg, "WARN", context)
}

/// Helper to format errors.
fn format_err<S: Into<String>>(msg: S, context: Option<&Context>) -> String {
    format_log(msg, "ERROR", context)
}

/// Helper to log info to STDERR.
pub fn log_info<S: Into<String>>(msg: S, context: Option<&Context>) {
    eprintln!("{}", format_info(msg, context));
}

/// Helper to log warnings to STDERR.
pub fn log_warn<S: Into<String>>(msg: S, context: Option<&Context>) {
    eprintln!("{}", format_warn(msg, context));
}

/// Helper to log errors to STDERR.
pub fn log_err<S: Into<String>>(msg: S, context: Option<&Context>) {
    eprintln!("{}", format_err(msg, context));
}

/// Represents a generic error in a makefile, including context.
#[derive(Debug)]
pub struct MakeError {
    pub msg: String,
    pub context: Context,
}

impl MakeError {
    pub fn new<S: Into<String>>(msg: S, context: Context) -> Self {
        Self {
            msg: msg.into(),
            context,
        }
    }
}

impl Error for MakeError {}

impl fmt::Display for MakeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format_err(&self.msg, Some(&self.context)))
    }
}
