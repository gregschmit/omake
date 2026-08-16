//! Generic logging facility with a default implementation.

pub mod context;

use context::Context;

pub const INFO: &str = "INFO";
pub const WARN: &str = "WARN";
pub const ERROR: &str = "ERROR";

const MAX_SEVERITY_LENGTH: usize = 5;

/// Generic trait any logger must implement.
pub trait Logger {
    /// Write the formatted message somewhere.
    fn write(&self, msg: String);

    /// The program-name prefix on each message. GNU make uses its own invocation name and, in a
    /// sub-make, a `[N]` recursion-level suffix (e.g. `make[1]`). The default has no level.
    fn prefix(&self) -> String {
        "make".to_string()
    }

    /// Log an `INFO` message.
    fn info(&self, msg: &str, context: Option<&Context>) {
        self.write(self.format_log(INFO, msg, context));
    }

    /// Print a build-status notice to stdout with the plain `prefix:` form and no level tag (e.g.
    /// "'X' is up to date." / "Nothing to be done for 'X'."), matching GNU make. Distinct from the
    /// stderr diagnostic path used by `info`/`warn`/`error`.
    fn notice(&self, msg: &str) {
        println!("{}: {msg}", self.prefix());
    }

    /// Log a `WARN` message.
    fn warn(&self, msg: &str, context: Option<&Context>) {
        self.write(self.format_log(WARN, msg, context));
    }

    /// Log an `ERROR` message.
    fn error(&self, msg: &str, context: Option<&Context>) {
        self.write(self.format_log(ERROR, msg, context));
    }

    /// Formatter for all log messages.
    fn format_log(&self, level: &str, msg: &str, context: Option<&Context>) -> String {
        // Format log level and context label/line.
        let level_display = format!("{level:width$}", width = MAX_SEVERITY_LENGTH);
        let context_label = context
            .and_then(|c| c.label())
            .map(|l| format!("[{l}] "))
            .unwrap_or_default();

        // Only show the context line if we are logging warnings or errors.
        let context_line = if level == WARN || level == ERROR {
            context
                .and_then(|c| c.display_line())
                .map(|l| format!("\n{l}"))
                .unwrap_or_default()
        } else {
            String::new()
        };

        // Return the formatted message.
        format!(
            "{}: {level_display} {context_label}| {msg}{context_line}",
            self.prefix()
        )
    }
}

/// Uses the default implementation and outputs to `stderr`. Carries the program's invocation name
/// and recursion level for the message prefix.
#[derive(Clone)]
pub struct DefaultLogger {
    /// The program's invocation name (argv[0] basename), used as the message prefix like GNU make.
    pub program: String,
    /// The recursion level (`MAKELEVEL`); a non-zero level adds a `[N]` suffix to the prefix.
    pub level: u32,
}

/// By default, print to `stderr`.
impl Logger for DefaultLogger {
    fn write(&self, msg: String) {
        eprintln!("{msg}");
    }

    fn prefix(&self) -> String {
        if self.level > 0 {
            format!("{}[{}]", self.program, self.level)
        } else {
            self.program.clone()
        }
    }
}
