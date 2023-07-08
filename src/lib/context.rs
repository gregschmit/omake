//! Simple implementation of a `Context` struct designed to track parsing/execution location.

use std::path::PathBuf;

/// Represents parsing/execution context, specifically, which file and where in the file something
/// is happening.
#[derive(Clone, Debug)]
pub struct Context {
    pub path: Option<PathBuf>,
    pub content: Option<String>,

    // Line/row number is determined when iterating the input, so we use `usize` here to match the
    // return type of `enumerate()`. Both line and row are `1`-indexed to match the convention other
    // programs (including other make implementations) use when referencing line/column numbers, so
    // `0` is a sentinel value indicating that the value is not set.
    pub line_index: Option<usize>,
    pub column_index: Option<usize>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            path: None,
            content: None,
            line_index: None,
            column_index: None,
        }
    }

    pub fn label(&self) -> Option<String> {
        let Some(path_display) = self.path.as_ref().map(|p| p.display() ) else {
            return None;
        };

        // Assume row index would only be set if line index is set. Also, increment indices by 1 to
        // convert to the traditional line/column display "number".
        return match self.line_index {
            Some(line) => match self.column_index {
                Some(column) => Some(format!("{}:{}:{}", path_display, line + 1, column + 1)),
                None => Some(format!("{}:{}", path_display, line + 1)),
            },
            None => Some(path_display.to_string()),
        };
    }

    pub fn display_line(&self) -> Option<String> {
        self.content.as_ref().map(|content| match self.line_index {
            Some(line) => {
                let line_s = (line + 1).to_string();
                let pad = " ".repeat(line_s.len());

                match self.column_index {
                    Some(column) => format!(
                        "{pad} |\n{line_number} | {content}\n{pad} | {caret_padding}^\n",
                        pad = pad,
                        line_number = line_s,
                        content = content,
                        caret_padding = " ".repeat(column),
                    ),
                    None => format!(
                        "{pad} |\n{line_number} | {content}\n{pad} | \n",
                        pad = pad,
                        line_number = line_s,
                        content = content,
                    ),
                }
            }
            None => format!(" | {content}\n", content = content),
        })
    }
}

impl From<PathBuf> for Context {
    fn from(path: PathBuf) -> Self {
        let mut context = Self::new();
        context.path = Some(path);
        context
    }
}
