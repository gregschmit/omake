use std::collections::HashMap;
use std::process::Command;

use super::{log_warn, Context, MakeError};

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

/// Wrapper for a mapping of targets to rules. WE also provide a facility to execute targets.
///
/// TODO: I would ideally like to have a `rule_storage` vector of rules, and then the `rule_lookup`
/// would map to rule refs rather than just rules. Currently, if a rule has 5 targets, then the rule
/// gets cloned 5 times.
#[derive(Debug)]
pub struct RuleMap {
    /// Maps targets to their rules.
    rule_lookup: HashMap<String, Vec<Rule>>,
}

impl RuleMap {
    pub fn new() -> Self {
        Self {
            rule_lookup: HashMap::new(),
        }
    }

    /// A helper to insert a rule and update the `rule_lookup`.
    pub fn insert(&mut self, rule: Rule) -> Result<(), MakeError> {
        // Load each rule_target into the `rule_lookup` table.
        for target in &rule.targets {
            match self.rule_lookup.get_mut(target) {
                Some(rules) => {
                    // Catch user mixing single and double-colon rules.
                    if let Some(first) = rules.first() {
                        if first.double_colon != rule.double_colon {
                            return Err(MakeError::new(
                                "Cannot define rules using `:` and `::` on the same target.",
                                rule.context.clone(),
                            ));
                        }
                    }

                    if rule.double_colon {
                        rules.push(rule.clone())
                    } else {
                        log_warn("Ignoring duplicate definition.", Some(&rule.context));
                    }
                }
                None => {
                    self.rule_lookup
                        .insert(target.to_owned(), vec![rule.clone()]);
                }
            }
        }

        Ok(())
    }

    /// Helper to execute the rules for a particular target, checking dependencies
    pub fn execute(&self, target: &String) -> Result<(), MakeError> {
        let rules = self.rule_lookup.get(target).ok_or(MakeError::new(
            format!("No rule to make target '{}'.", target),
            Context::new(),
        ))?;

        for rule in rules {
            rule.execute()?;
        }

        Ok(())
    }
}
