//! Error type for the parser/executor.

use std::error::Error;
use std::fmt;

use crate::logger::context::Context;

/// An error in the parsing or execution of a makefile.
#[derive(Debug)]
pub struct MakeError {
    pub msg: String,
    pub context: Context,
}

impl MakeError {
    pub fn new(msg: &str, context: Context) -> Self {
        Self {
            msg: msg.to_string(),
            context,
        }
    }
}

impl Error for MakeError {}

impl fmt::Display for MakeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}
