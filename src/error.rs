use std::error::Error;
use std::fmt;

use super::Context;

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
        write!(f, "[{}] Error: {}", self.context.line_number, self.msg)
    }
}
