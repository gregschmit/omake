use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::args::Args;
use crate::context::Context;
use crate::error::{log_info, log_warn, MakeError};

const SHELL: &str = "/bin/sh";
const SHELL_ARGS: &str = "-c";

/// Get the `mtime` of a file. Note that the return value also signals whether or not the file is
/// accessible, so a `None` value represents either the file not existing or the current user not
/// having the appropriate permissions to access the file.
///
/// TODO: Consider bailing on a file permissions issue? Not sure if POSIX specifies some behavior
/// here or if the major implementations halt execution on a permissions error.
fn get_mtime(file: &String, args: &Args) -> Option<SystemTime> {
    match fs::metadata(file) {
        Ok(metadata) => {
            if args.old_file.contains(file) {
                Some(UNIX_EPOCH)
            } else if args.new_file.contains(file) {
                // 1 year in the future.
                Some(SystemTime::now() + Duration::from_secs(365 * 24 * 60 * 60))
            } else {
                metadata.modified().ok()
            }
        }
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
    /// Unconditionally execute a rule.
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
#[derive(Debug)]
pub struct RuleMap {
    /// Storage for added rules. Rules must only be inserted, as removal may invalidate items in
    /// `by_target`.
    rules: Vec<Rule>,

    /// Map targets (strings) to the rules which reference them by index into `self.rules`.
    by_target: HashMap<String, Vec<usize>>,
}

/// Note that methods on `RuleMap` must ensure that only new entries are added to either `rules` or
/// `by_target` to ensure index references always remain valid. Also, entries added to `by_target`
/// must always initialize with at least one index, never an empty vector.
impl RuleMap {
    pub fn new() -> Self {
        Self {
            rules: vec![],
            by_target: HashMap::new(),
        }
    }

    /// Insert a rule, update the `by_target` hashmap, and validate the rule.
    pub fn insert(&mut self, rule: Rule) -> Result<(), MakeError> {
        // Load rule into the storage vector and get a reference to it and the insertion index.
        let index = self.rules.len();
        self.rules.push(rule);
        let rule = self.rules.last().unwrap();

        // Load each target into `by_target` hashmap and catch some basic validation errors.
        for target in &rule.targets {
            match self.by_target.get_mut(target) {
                Some(rule_indices) => {
                    // If there is an existing set of rules for this target, catch user mixing
                    // single and double-colon rules.
                    let first = &self.rules[rule_indices.first().unwrap().to_owned()];
                    if first.double_colon != rule.double_colon {
                        return Err(MakeError::new(
                            "Cannot define rules using `:` and `::` on the same target.",
                            rule.context.clone(),
                        ));
                    }

                    if rule.double_colon {
                        rule_indices.push(index);
                    } else {
                        log_warn("Ignoring duplicate definition.", Some(&rule.context));
                    }
                }
                None => {
                    self.by_target.insert(target.to_owned(), vec![index]);
                }
            }
        }

        Ok(())
    }

    /// Execute the rules for a particular target, checking prerequisites.
    pub fn execute(&self, target: &String, args: &Args, recursive: bool) -> Result<(), MakeError> {
        let rule_indices = self.by_target.get(target).ok_or_else(|| {
            MakeError::new(
                format!("No rule to make target '{}'.", target),
                Context::new(),
            )
        })?;
        let target_mtime_opt = get_mtime(target, args);

        // Old files have their rules ignored.
        if args.old_file.contains(target) {
            if !recursive {
                log_info(
                    format!("Target '{target}' is up to date (old)."),
                    Some(&Context::new()),
                );
            }
            return Ok(());
        }

        let mut executed = false;
        for i in rule_indices {
            let rule = &self.rules[i.to_owned()];
            let mut should_execute = args.always_make;

            // Check (and possibly execute) prereqs.
            for prereq in &rule.prerequisites {
                // Check if prereq exists unless `always_make`.
                if args.always_make {
                    self.execute(prereq, args, true)?;
                } else {
                    match get_mtime(prereq, args) {
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
                            self.execute(prereq, args, true)?;
                            should_execute = true;
                        }
                    }
                }
            }

            if target_mtime_opt.is_none() || should_execute {
                rule.execute()?;
                executed = true;
            }
        }

        if !executed {
            log_info(
                format!("Target '{target}' is up to date."),
                Some(&Context::new()),
            );
        }

        Ok(())
    }
}
