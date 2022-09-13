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
    rulemap: RuleMap,
    default_target: Option<String>,
}

impl Makefile {
    /// Principal interface for reading and parsing a makefile.
    pub fn new(makefile_fn: PathBuf) -> Result<Self, MakeError> {
        // Parse the makefile.
        let parser = Parser::from_file(makefile_fn)?;

        // Initialize and return the makefile.
        let makefile = Self {
            rulemap: parser.rulemap,
            default_target: parser.default_target,
        };
        Ok(makefile)
    }

    /// Principal interface for executing a parsed makefile, given a list of targets.
    pub fn execute(&self, mut targets: Vec<String>) -> Result<(), MakeError> {
        // Set targets list to default target if none were provided.
        if targets.len() == 0 {
            match &self.default_target {
                Some(t) => targets.push(t.clone()),
                None => {
                    return Err(MakeError::new(
                        "No target specified and no default target found.",
                        Context::new(),
                    ))
                }
            }
        }

        for target in targets {
            let rules = self.rulemap.get(&target).ok_or(MakeError::new(
                format!("No rule to make target '{}'.", &target),
                Context::new(),
            ))?;
            for rule in rules {
                println!("Would execute:\n{}\n---", rule.recipe.join("\n"));
            }
        }

        Ok(())
    }
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
