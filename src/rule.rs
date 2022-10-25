use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::time::SystemTime;

use super::{log_warn, Context, MakeError};

const SHELL: &str = "/bin/sh";
const SHELL_ARGS: &str = "-c";

/// Helper to get the `mtime` of a file as an optional value.
fn get_mtime(file: &String) -> Option<SystemTime> {
    match fs::metadata(file) {
        Ok(metadata) => metadata.modified().ok(),
        Err(_) => None,
    }
}

/// Represents a parsed rule from a makefile.
#[derive(Debug, Clone)]
pub struct Rule {
    pub targets: Vec<String>,
    pub prerequisites: Vec<String>,
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

/// Wrapper for a mapping of targets to rules. We also provide a facility to execute targets.
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
        // Load each target into the `rule_lookup` table.
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

    /// Helper to execute the rules for a particular target, checking prerequisites.
    pub fn execute(&self, target: &String) -> Result<(), MakeError> {
        let rules = self.rule_lookup.get(target).ok_or(MakeError::new(
            format!("No rule to make target '{}'.", target),
            Context::new(),
        ))?;
        let target_mtime_opt = get_mtime(target);

        for rule in rules {
            let mut should_execute = false;
            // Check (and possibly execute) prereqs.
            for prereq in &rule.prerequisites {
                // Check if prereq exists.
                match get_mtime(&prereq) {
                    Some(prereq_mtime) => {
                        // Prereq exists, so check if it's more up-to-date than the target.
                        if let Some(target_mtime) = target_mtime_opt {
                            if prereq_mtime > target_mtime {
                                should_execute = true;
                            }
                        }
                    }
                    None => {
                        // Prereq doesn't exist, so make it. By definition, it's more up-to-date
                        // than the target.
                        self.execute(prereq)?;
                        should_execute = true;
                    }
                }
            }

            if target_mtime_opt.is_none() || should_execute {
                rule.execute()?;
            }
        }

        Ok(())
    }
}
