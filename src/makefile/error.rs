use std::error::Error;
use std::fmt;

/// Represents an error in a makefile, providing metadata such as line number of the error.
#[derive(Debug)]
pub struct MakeError {
    pub msg: String,
    pub line_number: usize,
    // pub row_number: usize,
    // pub filename: String,
}
impl MakeError {
    pub fn new<S: Into<String>>(msg: S, line_number: usize) -> Self {
        Self {
            msg: msg.into(),
            line_number: line_number,
        }
    }
}
impl Error for MakeError {}
impl fmt::Display for MakeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[{}] Error: {}.", self.msg, self.line_number)
    }
}
