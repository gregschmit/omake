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
    let context_disp = match context {
        None => "".to_string(),
        Some(context) => format!("[{}] ", context.line_number),
    };

    // Format log level.
    let level_disp = format!("{:5} |", format!("{}", level));

    // Print the log message.
    format!("make: {} {}{}", level_disp, context_disp, msg.into())
}

/// Helper to format warnings.
fn format_warn<S: Into<String>>(msg: S, context: Option<&Context>) -> String {
    format!("{}", format_log(msg, "WARN", context))
}

/// Helper to format errors.
fn format_err<S: Into<String>>(msg: S, context: Option<&Context>) -> String {
    format!("{}", format_log(msg, "ERROR", context))
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
            context: context,
        }
    }
}

impl Error for MakeError {}

impl fmt::Display for MakeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format_err(&self.msg, Some(&self.context)))
    }
}
