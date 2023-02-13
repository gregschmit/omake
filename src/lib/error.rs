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

/// Get line content from a context.
/// TODO: improve the implementation of this function.
fn set_line_string_on_context(context: &mut Context) -> () {
    if let Some(path) = &context.path {
        return if let Ok(lines) = std::fs::read_to_string(path) {
            if let Some(line) = lines.lines().nth(context.line_number as usize) {
                context.line = Some(line.to_string());
            } else {
                context.line = None;
            }
        } else {
            context.line = None;
        };
    }
}

/// Helper to format info.
fn format_info(msg: impl AsRef<str>, context: Option<&Context>) -> String {
    format_log(msg, "INFO", context)
}

/// Helper to format warnings.
fn format_warn(msg: impl AsRef<str>, context: Option<&Context>) -> String {
    if let Some(context) = context {
        let mut context = context.clone();
        set_line_string_on_context(&mut context);
        format_log(msg, "WARN", Some(&context))
    } else {
        format_log(msg, "WARN", None)
    }
}

/// Helper to format errors.
fn format_err(msg: impl AsRef<str>, context: Option<&Context>) -> String {
    if let Some(context) = context {
        let mut context = context.clone();
        set_line_string_on_context(&mut context);
        format_log(msg, "ERROR", Some(&context))
    } else {
        format_log(msg, "ERROR", None)
    }
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
