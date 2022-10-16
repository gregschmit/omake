mod error;
mod expand;
mod rule;
mod vars;

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

pub use error::{log_err, log_warn};

use error::MakeError;
use expand::expand;
use rule::{Rule, RuleMap};
use vars::Vars;

const COMMENT_INDICATOR: char = '#';

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

/// The internal representation of a makefile.
#[derive(Debug)]
pub struct Makefile {
    rulemap: RuleMap,
    default_target: Option<String>,

    // Parser state.
    vars: Vars,
    current_rule: Option<Rule>,
    context: Context,
}

impl Makefile {
    /// Principal interface for reading and parsing a makefile.
    pub fn new(makefile_fn: PathBuf) -> Result<Self, MakeError> {
        // Initialize the `Makefile` struct with default values.
        let mut makefile = Self {
            rulemap: RuleMap::new(),
            default_target: None,
            vars: Vars::new([]),
            current_rule: None,
            context: Context::from_path(makefile_fn.clone()),
        };

        // Open the makefile and run it through the parser.
        let file = File::open(&makefile_fn).map_err(|e| {
            MakeError::new(
                format!("Could not read makefile ({}).", e),
                Context::from_path(makefile_fn),
            )
        })?;
        makefile.parse(BufReader::new(file))?;

        Ok(makefile)
    }

    /// This helper is designed to iterate over the makefile lines, call `parse_line` to handle the
    /// actual parsing logic, and manage context.
    fn parse<R: BufRead>(&mut self, stream: R) -> Result<(), MakeError> {
        self.current_rule = None;

        for (i, result) in stream.lines().enumerate() {
            // Set the context line number and extract the line.
            self.context.line_number = i + 1;
            let line = result.map_err(|e| MakeError::new(e.to_string(), self.context.clone()))?;

            // Parse the line.
            self.parse_line(line)?;
        }

        // Always push a blank line at the end to terminate trailing rules.
        self.parse_line("".to_string())?;

        Ok(())
    }

    /// The line parser is where the "meat" of the parsing occurs. This is responsible for
    /// extracting rules from the physical lines of the makefile stream, properly handling escaped
    /// newlines and semicolons, and also managing state, such as variable assignments and
    /// annotating when the parser moves in-to and out-of a rule definition.
    fn parse_line(&mut self, line: String) -> Result<(), MakeError> {
        // Handle recipe lines.
        let recipe_prefix = &self.vars.get(".RECIPEPREFIX").value;
        if line.starts_with(recipe_prefix) {
            // If line starts with the recipe prefix, then push it to the current rule.
            match &mut self.current_rule {
                None => return Err(MakeError::new("recipe without rule", self.context.clone())),
                Some(r) => {
                    // Strip the recipe prefix first.
                    let cmd = line
                        .strip_prefix(recipe_prefix)
                        .expect("line known to start with a recipe prefix")
                        .trim()
                        .to_string();

                    if !cmd.is_empty() {
                        r.recipe.push(
                            expand(cmd.as_str(), &self.vars)
                                .map_err(|e| MakeError::new(e, self.context.clone()))?,
                        );
                    }
                }
            }
            return Ok(());
        }

        // Anything other than recipe lines terminate a rule definition.
        if let Some(rule) = self.current_rule.take() {
            for target in rule.targets.iter() {
                // Set default target if none is specified and this is a normal target. Note that
                // `unwrap()` here is safe because the target is a result of splitting on
                // whitespace, which would result in an empty array if there is only whitespace or
                // no text.
                if self.default_target.is_none() && target.chars().nth(0).unwrap() != '.' {
                    self.default_target = Some(target.clone());
                }

                // Finish terminating this rule definition (adding to rulemap).
                match self.rulemap.get_mut(target) {
                    Some(existing_rules) => {
                        // Catch user mixing single and double-colon rules.
                        if let Some(first) = existing_rules.first() {
                            if first.double_colon != rule.double_colon {
                                return Err(MakeError::new(
                                    "Cannot define rules using `:` and `::` on the same target.",
                                    self.context.clone(),
                                ));
                            }
                        }

                        if rule.double_colon {
                            existing_rules.push(rule.clone())
                        } else {
                            log_warn("Ignoring duplicate definition.", Some(&self.context));
                        }
                    }
                    None => {
                        self.rulemap.insert(target.to_owned(), vec![rule.clone()]);
                    }
                }
            }
        }

        // Ignore comments and blank lines.
        if line.starts_with(COMMENT_INDICATOR) || line.trim().is_empty() {
            return Ok(());
        }

        // Handle rule definitions.
        if let Some((targets, mut deps)) = line.split_once(':') {
            // First, if deps start with another `:`, then this is a double-colon rule, so we should
            // mark it as such.
            let mut double_colon = false;
            if let Some(ch) = deps.chars().next() {
                if ch == ':' {
                    deps = &deps[1..];
                    double_colon = true;
                }
            }

            // There could be a semicolon after dependencies, in which case we should
            // parse everything after that as a rule line.
            let rule = deps.split_once(';').map(|(d, r)| {
                deps = d;
                r
            });

            self.current_rule = Some(Rule {
                targets: expand(targets, &self.vars)
                    .map_err(|e| MakeError::new(e, self.context.clone()))?
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                dependencies: expand(deps, &self.vars)
                    .map_err(|e| MakeError::new(e, self.context.clone()))?
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                recipe: vec![],
                context: self.context.clone(),
                double_colon: double_colon,
            });

            // Add rule line if we found one.
            if let Some(r) = rule {
                self.parse_line(format!("{}{}", self.vars.get(".RECIPEPREFIX").value, r))?;
            }

            return Ok(());
        }

        // Handle variable assignments.
        if let Some((k, v)) = line.split_once('=') {
            if let Err(e) = self.vars.set(
                k,
                &expand(v.trim_start(), &self.vars)
                    .map_err(|e| MakeError::new(e, self.context.clone()))?,
                false,
            ) {
                return Err(MakeError::new(e, self.context.clone()));
            };
            return Ok(());
        }

        // Otherwise, throw error if line is not recognizable.
        return Err(MakeError::new("Invalid line type.", self.context.clone()));
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

            // TODO: Replace all of this with rule executor.
            for rule in rules {
                for line in rule.recipe.iter() {
                    const SHELL: &str = "/bin/sh";
                    const SHELL_ARGS: &str = "-c";
                    use std::process::Command;
                    println!("{}", line);
                    let _ = Command::new(SHELL).arg(SHELL_ARGS).arg(line).spawn();
                }
            }
        }

        Ok(())
    }
}
