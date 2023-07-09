//! Generic logging facility with a default implementation.

use crate::context::Context;

pub const INFO: &str = "INFO";
pub const WARN: &str = "WARN";
pub const ERROR: &str = "ERROR";

const MAX_SEVERITY_LENGTH: usize = 5;

/// Generic trait any logger must implement.
pub trait Logger {
    /// Write the message somewhere.
    fn write(&self, msg: String);

    /// Log an `INFO` message.
    fn info(&self, msg: impl AsRef<str>, context: Option<&Context>) {
        self.write(self.format_log(INFO, msg, context));
    }

    /// Log a `WARN` message.
    fn warn(&self, msg: impl AsRef<str>, context: Option<&Context>) {
        self.write(self.format_log(WARN, msg, context));
    }

    /// Log an `ERROR` message.
    fn error(&self, msg: impl AsRef<str>, context: Option<&Context>) {
        self.write(self.format_log(ERROR, msg, context));
    }

    /// Formatter for all log messages.
    fn format_log(&self, level: &str, msg: impl AsRef<str>, context: Option<&Context>) -> String {
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
}

/// Uses the default implementation and outputs to `stderr`.
pub struct DefaultLogger {}

/// By default, print to `stderr`.
impl Logger for DefaultLogger {
    fn write(&self, msg: String) {
        eprintln!("{}", msg);
    }
}
