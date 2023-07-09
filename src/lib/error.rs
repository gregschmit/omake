//! Error type for the parser/executor.

use std::error::Error;
use std::fmt;

use crate::context::Context;

/// An error in the parsing or execution of a makefile.
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

/// Not really used, but needed so `MakeError` can implement `Error`.
impl fmt::Display for MakeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{e:?}", e = &self)
    }
}
