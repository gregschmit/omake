//! Simple implementation of a `Context` structure designed to track parsing/execution location.

use std::path::PathBuf;

/// Represents parsing/execution context, specifically, which file and where in the file something
/// is happening.
#[derive(Clone, Debug)]
pub struct Context {
    pub path: Option<PathBuf>,
    pub line_number: usize,
    // pub row_number: Option(usize),
}

impl Context {
    pub fn new() -> Self {
        Self {
            path: None,
            line_number: 0,
        }
    }
}

impl From<PathBuf> for Context {
    fn from(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            line_number: 0,
        }
    }
}
