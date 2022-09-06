mod error;
mod parser;
mod rule;

use std::fs::File;
use std::io::BufReader;

use error::MakeError;
use parser::Parser;
use rule::Rule;

/// The internal representation of a makefile.
#[derive(Debug)]
pub struct Makefile {
    rules: Vec<Rule>,
}

impl Makefile {
    pub fn new(makefile_stream: BufReader<File>) -> Result<Self, MakeError> {
        let mut makefile = Self { rules: vec![] };

        let parser = Parser::new(makefile_stream)?;
        makefile.rules = parser.rules;
        Ok(makefile)
    }
}

/// Represents parsing/execution context. This is just the line number for now, but will include
/// the makefile path (important when implementing `include`) and the row number.
#[derive(Clone, Debug)]
pub struct Context {
    pub line_number: usize,
    // pub row_number: Option(usize),
    // pub path: String,
}

impl Context {
    pub fn new() -> Self {
        Context { line_number: 0 }
    }
}
