//! Error type for the parser/executor.

use std::error::Error;
use std::fmt;

use crate::logger::context::Context;

/// An error in the parsing or execution of a makefile.
#[derive(Debug)]
pub struct MakeError {
    pub msg: String,
    pub context: Context,
    /// A pre-formatted line to print verbatim, used by `--compat` to emit a gmake/bmake-exact
    /// diagnostic. When `None`, the error renders in omake's native `ERROR [file:N] | … caret` form.
    pub display: Option<String>,
}

impl MakeError {
    pub fn new(msg: &str, context: Context) -> Self {
        Self {
            msg: msg.to_string(),
            context,
            display: None,
        }
    }

    /// Attach a pre-formatted `--compat` line (see [`MakeError::display`]).
    pub fn with_display(mut self, line: String) -> Self {
        self.display = Some(line);
        self
    }
}

impl Error for MakeError {}

impl fmt::Display for MakeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}
