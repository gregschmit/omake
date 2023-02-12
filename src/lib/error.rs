use std::error::Error;
use std::fmt;

use super::Context;

/// Formatter for all log messages.
fn format_log(msg: impl AsRef<str>, level: &str, context: Option<&Context>) -> String {
    // Format log level.
    let level_display = format!("{:5}", level);

    // Format context.
    let context_display = match context {
        None => String::new(),
        Some(context) => match &context.path {
            None => String::new(),
            Some(path) => {
                if context.line_number == 0 {
                    format!("[{}:{}] ", path.display(), context.line_number)
                } else {
                    format!("[{}] ", path.display())
                }
            }
        },
    };

    // Print the log message.
    format!("make: {level_display} {context_display}| {}", msg.as_ref())
}

/// Helper to format info.
fn format_info(msg: impl AsRef<str>, context: Option<&Context>) -> String {
    format_log(msg, "INFO", context)
}

/// Helper to format warnings.
fn format_warn(msg: impl AsRef<str>, context: Option<&Context>) -> String {
    format_log(msg, "WARN", context)
}

/// Helper to format errors.
fn format_err(msg: impl AsRef<str>, context: Option<&Context>) -> String {
    format_log(msg, "ERROR", context)
}

/// Helper to log info to STDERR.
pub fn log_info(msg: impl AsRef<str>, context: Option<&Context>) {
    eprintln!("{}", format_info(msg, context));
}

/// Helper to log warnings to STDERR.
pub fn log_warn(msg: impl AsRef<str>, context: Option<&Context>) {
    eprintln!("{}", format_warn(msg, context));
}

/// Helper to log errors to STDERR.
pub fn log_err(msg: impl AsRef<str>, context: Option<&Context>) {
    eprintln!("{}", format_err(msg, context));
}

/// Represents a generic error in a makefile, including context.
#[derive(Debug)]
pub struct MakeError {
    pub msg: String,
    pub context: Context,
}

impl MakeError {
    pub fn new(msg: impl AsRef<str>, context: Context) -> Self {
        Self {
            msg: msg.as_ref().to_string(),
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
