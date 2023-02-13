//! Simple implementation of a `Context` structure designed to track parsing/execution location.

use std::path::PathBuf;

/// Represents parsing/execution context, specifically, which file and where in the file something
/// is happening.
#[derive(Clone, Debug)]
pub struct Context {
    pub path: Option<PathBuf>,
    pub line_number: u64,
    pub row_number: Option<u64>,
    pub line: Option<String>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            path: None,
            line_number: 0,
            row_number: None,
            line: None,
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl From<PathBuf> for Context {
    fn from(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            line_number: 0,
            row_number: None,
            line: None,
        }
    }
}
