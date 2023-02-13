mod context;
mod error;
mod expand;
mod rule_map;
mod vars;

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

pub use context::Context;
pub use error::{log_err, log_info, log_warn};

use error::MakeError;
use expand::expand;
use rule_map::{Rule, RuleMap};
use vars::Vars;

const COMMENT_INDICATOR: char = '#';

/// An ordered list of files which ought to be used to search for a makefile.
pub const MAKEFILE_SEARCH: [&str; 6] = [
    "Makefile",
    "makefile",
    "BSDMakefile",
    "BSDmakefile",
    "GNUMakefile",
    "GNUmakefile",
];

/// A representation of the options that may be provided to the `Makefile` struct. We keep this
/// separate from the `Args` struct on the binary side so this library may be used without requiring
/// the use of `clap`.
#[derive(Debug, Default)]
pub struct Opts {
    pub always_make: bool,
    pub old_files: Vec<String>,
    pub new_files: Vec<String>,
}

/// The internal representation of a makefile.
#[derive(Debug)]
pub struct Makefile {
    opts: Opts,
    rule_map: RuleMap,
    default_target: Option<String>,

    // Parser state.
    vars: Vars,
    current_rule: Option<Rule>,
    context: Context,
}

impl Makefile {
    /// Principal interface for reading and parsing a makefile.
    pub fn new(makefile_fn: PathBuf, opts: Opts) -> Result<Self, MakeError> {
        // Initialize the `Makefile` struct with default values.
        let mut makefile = Self {
            opts,
            rule_map: RuleMap::new(),
            default_target: None,
            vars: Vars::new([]),
            current_rule: None,
            context: makefile_fn.clone().into(),
        };

        // Open the makefile and run it through the parser.
        let file = File::open(&makefile_fn).map_err(|e| {
            MakeError::new(
                format!("Could not read makefile ({}).", e),
                makefile_fn.into(),
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
            self.context.line_number = (i + 1) as u64;
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
            // If there is no default target, see if we can assign one.
            if self.default_target.is_none() {
                for target in rule.targets.iter() {
                    // Set default target if none is specified and this is a normal target.
                    if self.default_target.is_none() && !target.starts_with('.') {
                        self.default_target = Some(target.clone());
                    }
                }
            }

            // Add the rule to the `rule_map`.
            self.rule_map.insert(rule)?;
        }

        // Ignore comments and blank lines.
        let trimmed_line = line.trim();
        if trimmed_line.starts_with(COMMENT_INDICATOR) || trimmed_line.is_empty() {
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

            // There could be a semicolon after prerequisites, in which case we should parse
            // everything after that as a rule line.
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
                prerequisites: expand(deps, &self.vars)
                    .map_err(|e| MakeError::new(e, self.context.clone()))?
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                recipe: vec![],
                context: self.context.clone(),
                double_colon,
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
        Err(MakeError::new("Invalid line type.", self.context.clone()))
    }

    /// Principal interface for executing a parsed makefile, given a list of targets.
    pub fn execute(&self, mut targets: Vec<String>) -> Result<(), MakeError> {
        // Set targets list to default target if none were provided.
        if targets.is_empty() {
            match &self.default_target {
                Some(t) => targets.push(t.clone()),
                None => {
                    return Err(MakeError::new(
                        "No target specified and no default target found.",
                        Context {
                            path: None,
                            line_number: 0,
                            row_number: None,
                            line: None,
                        },
                    ))
                }
            }
        }

        for target in targets {
            self.rule_map.execute(&target, &self.opts, false)?;
        }

        Ok(())
    }
}
