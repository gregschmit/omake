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

    pub fn label(&self) -> Option<String> {
        self.path.as_ref().map(|path| {
            if self.line_number == 0 {
                if self.column_number == 0 {
                    format!("{}:{}", path.display(), self.line_number)
                } else {
                    format!(
                        "{}:{}:{}",
                        path.display(),
                        self.line_number,
                        self.column_number
                    )
                }
            } else {
                path.display().to_string()
            }
        })
    }

    pub fn display_line(&self) -> Option<String> {
        self.line.as_ref().map(|line| {
            let line_number_s = if self.line_number == 0 {
                String::new()
            } else {
                self.line_number.to_string()
            };
            let pad = " ".repeat(line_number_s.len());
            let caret = String::new();
            format!(
                "{pad} |\n{line_number} | {line}\n{pad} | {caret}\n",
                pad = pad,
                line_number = line_number_s,
                line = line,
                caret = caret,
            )
        })
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
