use std::collections::HashMap;
use std::process::Command;

use super::{Context, Logger, MakeError, Makefile};

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
    pub fn execute<L: Logger>(&self, makefile: &Makefile<L>) -> Result<(), MakeError> {
        let shell = &makefile.vars.get("SHELL").value;
        let shell_flags = makefile
            .vars
            .get(".SHELLFLAGS")
            .value
            .split_whitespace()
            .collect::<Vec<_>>();

        for line in self.recipe.iter() {
            // Determine if the first character is a command modifier.
            let command_modifier = match line.chars().next().unwrap() {
                ch @ ('@' | '-' | '+') => Some(ch),
                _ => None,
            };

            // Echo the line to stdout, unless suppressed.
            if command_modifier != Some('@') || makefile.opts.just_print {
                println!("{}", line);

                // If we're just printing, we are done with this line.
                if makefile.opts.just_print {
                    continue;
                }
            }

            // Execute the recipe line.
            let res = Command::new(shell)
                .args(&shell_flags)
                .arg(line)
                .status()
                .map_err(|e| MakeError::new(e.to_string(), self.context.clone()))?;

            // Check for command errors, unless directed to ignore them.
            if command_modifier != Some('-') && !makefile.opts.ignore_errors {
                if let Some(code) = res.code() {
                    if code != 0 {
                        return Err(MakeError::new(
                            format!("Failed with code {}.", code),
                            self.context.clone(),
                        ));
                    }
                } else {
                    return Err(MakeError::new("Killed.", self.context.clone()));
                }
            }
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
    pub fn insert<L: Logger>(&mut self, rule: Rule, logger: &Box<L>) -> Result<(), MakeError> {
        // Load rule into the storage vector and get a reference to it and the insertion index.
        let index = self.rules.len();
        self.rules.push(rule);
        let rule = self.rules.last().unwrap();

        // Load each target into `by_target` hashmap and catch some basic validation errors.
        for target in &rule.targets {
            match self.by_target.get_mut(target) {
                None => {
                    self.by_target.insert(target.to_owned(), vec![index]);
                }
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
                        logger.warn(
                            "Ignoring duplicate definition.".to_string(),
                            Some(&rule.context),
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute the rules for a particular target, checking prerequisites.
    pub fn execute<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &String,
    ) -> Result<(), MakeError> {
        let rule_indices = self.by_target.get(target).ok_or_else(|| {
            MakeError::new(
                format!("No rule to make target '{}'.", target),
                Context::new(),
            )
        })?;
        let target_mtime_opt = makefile.get_mtime(target);

        // Old files have their rules ignored.
        if makefile.opts.old_file.contains(target) {
            makefile.logger.info(
                format!("Target '{target}' is up to date (old)."),
                Some(&Context::new()),
            );
            return Ok(());
        }

        let mut executed = false;
        for i in rule_indices {
            let rule = &self.rules[i.to_owned()];
            let mut should_execute = makefile.opts.always_make;

            // Check (and possibly execute) prereqs.
            for prereq in &rule.prerequisites {
                // Check if prereq exists unless `always_make`.
                if makefile.opts.always_make {
                    self.execute(makefile, prereq)?;
                } else {
                    match makefile.get_mtime(prereq) {
                        None => {
                            // Prereq doesn't exist, so make it. By definition, it's more up-to-date
                            // than the target.
                            self.execute(makefile, prereq)?;
                            should_execute = true;
                        }
                        Some(prereq_mtime) => {
                            // Prereq exists, so check if it's more up-to-date than the target.
                            if let Some(target_mtime) = target_mtime_opt {
                                if prereq_mtime > target_mtime {
                                    should_execute = true;
                                }
                            }
                        }
                    }
                }
            }

            if target_mtime_opt.is_none() || should_execute {
                rule.execute(makefile)?;
                executed = true;
            }
        }

        if !executed {
            makefile.logger.info(
                format!("Target '{target}' is up to date."),
                Some(&Context::new()),
            );
        }

        Ok(())
    }
}
