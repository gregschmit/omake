use std::error::Error;
use std::fmt;

use crate::context::Context;
use crate::logger::{DefaultLogger, Logger, ERROR};

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
        write!(
            f,
            "{}",
            DefaultLogger {}.format_log(&self.msg, ERROR, Some(&self.context))
        )
    }
}
