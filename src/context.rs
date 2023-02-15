//! Simple implementation of a `Context` struct designed to track parsing/execution location.

use std::path::PathBuf;

/// Represents parsing/execution context, specifically, which file and where in the file something
/// is happening.
#[derive(Clone, Debug)]
pub struct Context {
    pub path: Option<PathBuf>,

    // Line/row number is determined when iterating the input, so we use `usize` here to match the
    // return type of `enumerate()`. Both line and row are `1`-indexed to match the convention other
    // programs (including other make implementations) use when referencing line/column numbers, so
    // `0` is a sentinel value indicating that the value is not set.
    pub line_number: usize,
    pub column_number: usize,

    pub line: Option<String>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            path: None,
            line_number: 0,
            column_number: 0,
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
            column_number: 0,
            line: None,
        }
    }
}
