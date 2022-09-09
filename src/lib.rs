mod error;
mod parser;
mod rule;

use std::path::PathBuf;

pub use error::{log_err, log_warn};

use error::MakeError;
use parser::Parser;
use rule::{Rule, RuleMap};

/// The internal representation of a makefile.
#[derive(Debug)]
pub struct Makefile {
    ruleset: RuleMap,
}

impl Makefile {
    /// Principal interface for reading and parsing a makefile.
    pub fn new(makefile_fn: PathBuf) -> Result<Self, MakeError> {
        // Parse the makefile.
        let parser = Parser::from_file(makefile_fn)?;

        // Initialize and return the makefile.
        let makefile = Self {
            ruleset: parser.ruleset,
        };
        Ok(makefile)
    }

    // pub fn execute(&self) -> Result<(), MakeError> {}
}

/// Represents parsing/execution context.
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

    pub fn from_path(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            line_number: 0,
        }
    }
}
