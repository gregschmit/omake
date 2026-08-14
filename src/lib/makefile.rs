//! The core logic for parsing and executing makefiles.

pub mod logical_line;
pub mod opts;
pub mod rule_map;

use opts::Opts;

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, fs::File};

use crate::error::MakeError;
use crate::expand::expand;
use crate::logger::{Logger, context::Context};
use crate::vars::Vars;

use rule_map::{Rule, RuleMap};

const COMMENT_INDICATOR: char = '#';

/// The primary interface for reading, parsing, and executing a makefile.
#[derive(Debug)]
pub struct Makefile<L: Logger> {
    pub opts: Opts,
    pub logger: Box<L>,

    rule_map: RuleMap,
    default_target: Option<String>,

    // Parser state.
    pub vars: Vars,
    current_rule: Option<Rule>,
    context: Context,
}

impl<L: Logger> Makefile<L> {
    /// Principal interface for reading and parsing a makefile.
    pub fn new(path: PathBuf, opts: Opts, vars: Vars, logger: L) -> Result<Self, MakeError> {
        // Initialize the `Makefile` struct with default values.
        let mut makefile = Self {
            opts,
            logger: Box::new(logger),
            rule_map: RuleMap::new(),
            default_target: None,
            vars,
            current_rule: None,
            context: path.clone().into(),
        };

        // Open the makefile and run it through the parser.
        let file = File::open(&path).map_err(|e| {
            MakeError::new(&format!("Could not read makefile ({}).", e), path.into())
        })?;
        makefile.parse(BufReader::new(file))?;

        Ok(makefile)
    }

    /// Iterate over the makefile's lines, call `parse_line` to handle the actual parsing logic, and
    /// manage context.
    fn parse<R: BufRead>(&mut self, input: R) -> Result<(), MakeError> {
        self.current_rule = None;

        // TODO(logical-line-parser): join physical lines ending in an unescaped backslash into
        // logical lines before handing them to `parse_line`. See `logical_line.rs` and TODO.md.
        for (i, result) in input.lines().enumerate() {
            // Set the context line number, and clear the content until we successfully read the
            // line, so a read error (e.g. invalid UTF-8) doesn't report the previous line's text.
            self.context.line_index = Some(i);
            self.context.content = None;
            let line = result.map_err(|e| MakeError::new(&e.to_string(), self.context.clone()))?;
            self.context.content = Some(line.clone());

            // Parse the line.
            self.parse_line(line)?;
        }

        // Always push two blank lines at the end to terminate trailing rules, even if the last rule
        // contained a trailing backslash.
        self.parse_line("".to_string())?;
        self.parse_line("".to_string())?;

        Ok(())
    }

    /// The line parser is where the "meat" of the parsing occurs. This is responsible for
    /// extracting rules from the physical lines of the makefile input, properly handling escaped
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
                            expand(cmd.as_str(), &self.vars).map_err(|e| {
                                MakeError::new(&e.to_string(), self.context.clone())
                            })?,
                        );
                    }
                }
            }
            return Ok(());
        }

        // Strip inline comments (`#` to end of line) on non-recipe lines. Recipe lines keep `#` so
        // it can reach the shell, and they already returned above.
        let line = strip_inline_comment(&line);

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
            self.rule_map.insert(rule, &*self.logger)?;
        }

        // Ignore pure comments and blank lines.
        let trimmed_line = line.trim();
        if trimmed_line.starts_with(COMMENT_INDICATOR) || trimmed_line.is_empty() {
            return Ok(());
        }

        // Handle rule definitions.
        if let Some((targets, mut deps)) = line.split_once(':') {
            // First, if deps start with another `:`, then this is a double-colon rule, so we should
            // mark it as such.
            let mut double_colon = false;
            if let Some(ch) = deps.chars().next()
                && ch == ':'
            {
                deps = &deps[1..];
                double_colon = true;
            }

            // There could be a semicolon after prerequisites, in which case we should parse
            // everything after that as a rule line.
            let rule = deps.split_once(';').map(|(d, r)| {
                deps = d;
                r
            });

            self.current_rule = Some(Rule {
                targets: expand(targets, &self.vars)
                    .map_err(|e| MakeError::new(&e, self.context.clone()))?
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                prerequisites: expand(deps, &self.vars)
                    .map_err(|e| MakeError::new(&e, self.context.clone()))?
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                recipe: vec![],
                context: self.context.clone(),
                double_colon,
            });

            // Add rule line if we found one.
            if let Some(r) = rule {
                self.parse_line(format!("{}{r}", self.vars.get(".RECIPEPREFIX").value))?;
            }

            return Ok(());
        }

        // Handle variable assignments.
        if let Some((k, v)) = line.split_once('=') {
            if let Err(e) = self.vars.set(
                k,
                &expand(v.trim_start(), &self.vars)
                    .map_err(|e| MakeError::new(&e, self.context.clone()))?,
                false,
            ) {
                return Err(MakeError::new(&e, self.context.clone()));
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
                None => {
                    return Err(MakeError::new(
                        "No target specified and no default target found.",
                        Context::new(),
                    ));
                }
                Some(t) => targets.push(t.clone()),
            }
        }

        for target in targets {
            self.rule_map.execute(self, &target)?;
        }

        Ok(())
    }

    /// Get the `mtime` of a file. Note that the return value also signals whether or not the file
    /// is accessible, so a `None` value represents either the file not existing or the current user
    /// not having the appropriate permissions to access the file.
    ///
    /// TODO: Consider bailing on a file permissions issue? Not sure if POSIX specifies some
    /// behavior here or if the major implementations halt execution on a permissions error.
    fn get_mtime(&self, file: &String) -> Option<SystemTime> {
        match fs::metadata(file) {
            Ok(metadata) => {
                if self.opts.old_file.contains(file) {
                    Some(UNIX_EPOCH)
                } else if self.opts.new_file.contains(file) {
                    // 1 year in the future.
                    Some(SystemTime::now() + Duration::from_secs(365 * 24 * 60 * 60))
                } else {
                    metadata.modified().ok()
                }
            }
            Err(_) => None,
        }
    }
}

/// Remove an inline comment (from the first unescaped `#` to end of line). An escaped `\#` becomes
/// a literal `#`; other backslash sequences are left untouched. This must not be used on recipe
/// lines, where `#` is significant and passed through to the shell.
fn strip_inline_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();

    while let Some(c) = chars.next() {
        match c {
            '#' => break,
            '\\' => match chars.next() {
                Some('#') => out.push('#'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            _ => out.push(c),
        }
    }

    out
}
