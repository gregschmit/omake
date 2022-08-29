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
