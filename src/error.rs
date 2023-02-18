use std::error::Error;
use std::fmt;

use crate::context::Context;

const INFO: &str = "INFO";
const WARN: &str = "WARN";
const ERROR: &str = "ERROR";
const MAX_SEVERITY_LENGTH: usize = 5;

/// Formatter for all log messages.
fn format_log(msg: impl AsRef<str>, level: &str, context: Option<&Context>) -> String {
    // Format log level and context label/line.
    let level_display = format!("{:0width$}", level, width = MAX_SEVERITY_LENGTH);
    let context_label = context
        .and_then(|c| c.label())
        .map(|l| format!("[{}] ", l))
        .unwrap_or_default();

    // Only show the context line if we are logging warnings or errors.
    let context_line = if level == "WARN" || level == "ERROR" {
        context
            .and_then(|c| c.display_line())
            .map(|l| format!("\n{}", l))
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Return the formatted message.
    format!(
        "make: {level_display} {context_label}| {}{}",
        msg.as_ref(),
        context_line
    )
}

/// Log an `INFO` message to STDERR.
pub fn log_info(msg: impl AsRef<str>, context: Option<&Context>) {
    eprintln!("{}", format_log(msg, INFO, context));
}

/// Log a `WARN` message to STDERR.
pub fn log_warn(msg: impl AsRef<str>, context: Option<&Context>) {
    eprintln!("{}", format_log(msg, WARN, context));
}

/// Log an `ERROR` message to STDERR.
pub fn log_error(msg: impl AsRef<str>, context: Option<&Context>) {
    eprintln!("{}", format_log(msg, ERROR, context));
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
        write!(f, "{}", format_log(&self.msg, ERROR, Some(&self.context)))
    }
}
