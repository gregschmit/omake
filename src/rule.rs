use std::collections::HashMap;
use std::process::Command;

use super::{Context, MakeError};

const SHELL: &str = "/bin/sh";
const SHELL_ARGS: &str = "-c";

/// Represents a parsed rule from a makefile.
#[derive(Debug, Clone)]
pub struct Rule {
    pub targets: Vec<String>,
    pub dependencies: Vec<String>,
    pub recipe: Vec<String>,
    pub context: Context,
    pub double_colon: bool,
}

impl Rule {
    /// Helper to unconditionally execute a rule.
    pub(super) fn execute(&self) -> Result<(), MakeError> {
        for line in self.recipe.iter() {
            // Echo the line to stdout.
            println!("{}", line);

            // Run line in the shell.
            Command::new(SHELL)
                .arg(SHELL_ARGS)
                .arg(line)
                .status()
                .map_err(|e| MakeError::new(e.to_string(), self.context.clone()))?;
        }

        Ok(())
    }
}

/// Mapping of individual targets to vectors of rules which reference the target.
pub type RuleMap = HashMap<String, Vec<Rule>>;

// impl RuleMap {
//     /// Helper to execute the rules for a particular target, checking dependencies
//     // pub fn execute(&self) -> Result<(), MakeError> {
//     //     let rules = self.rulemap.get(&target).ok_or(MakeError::new(
//     //         format!("No rule to make target '{}'.", &target),
//     //         Context::new(),
//     //     ))?;
//     //     for rule in rules {
//     //         rule.execute();
//     //     }
//     // }
// }
