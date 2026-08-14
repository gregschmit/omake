//! The core logic for parsing and executing makefiles.

pub mod logical_line;
pub mod opts;
pub mod rule_map;

use opts::Opts;

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Command;
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

        // Join physical lines ending in an unescaped backslash into logical lines. A non-recipe
        // continuation collapses the backslash/newline (and surrounding whitespace) into a single
        // space; a recipe continuation keeps the backslash and newline (so the shell sees the
        // continuation) but strips the recipe prefix from each continuation line.
        let mut pending: Option<PendingLine> = None;

        for (i, result) in input.lines().enumerate() {
            // Set the context line number, and clear the content until we successfully read the
            // line, so a read error (e.g. invalid UTF-8) doesn't report the previous line's text.
            self.context.line_index = Some(i);
            self.context.content = None;
            let physical =
                result.map_err(|e| MakeError::new(&e.to_string(), self.context.clone()))?;
            let continues = ends_with_odd_backslash(&physical);

            // Extend the pending logical line, or start a new one.
            let p = match pending.take() {
                Some(mut p) => {
                    if p.is_recipe {
                        // Keep the backslash/newline; drop only the continuation's recipe prefix.
                        let cont = physical
                            .strip_prefix(&p.recipe_prefix)
                            .unwrap_or(&physical)
                            .to_string();
                        p.accum.push('\n');
                        p.accum.push_str(&cont);
                    } else {
                        let content = if continues {
                            strip_one_trailing_backslash(&physical)
                        } else {
                            physical.clone()
                        };
                        let trimmed_len = p.accum.trim_end().len();
                        p.accum.truncate(trimmed_len);
                        p.accum.push(' ');
                        p.accum.push_str(content.trim_start());
                    }
                    p
                }
                None => {
                    let recipe_prefix = self.vars.get(".RECIPEPREFIX").value.to_string();
                    let is_recipe = physical.starts_with(&recipe_prefix);
                    let accum = if is_recipe || !continues {
                        physical.clone()
                    } else {
                        strip_one_trailing_backslash(&physical)
                    };
                    PendingLine {
                        start_index: i,
                        is_recipe,
                        recipe_prefix,
                        accum,
                    }
                }
            };

            if continues {
                pending = Some(p);
            } else {
                self.context.line_index = Some(p.start_index);
                self.context.content = Some(p.accum.clone());
                self.parse_line(p.accum)?;
            }
        }

        // A continuation left dangling at end of input is still parsed.
        if let Some(p) = pending {
            self.context.line_index = Some(p.start_index);
            self.context.content = Some(p.accum.clone());
            self.parse_line(p.accum)?;
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
                    // Strip the recipe prefix and store the command RAW; recipes are expanded at
                    // execution time (so later variable definitions and automatic variables like
                    // `$@` resolve). Skip blank recipe lines.
                    let cmd = line
                        .strip_prefix(recipe_prefix)
                        .expect("line known to start with a recipe prefix");

                    if !cmd.trim().is_empty() {
                        r.recipe.push(cmd.to_string());
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

        // Handle variable assignments. This must run before rule detection so operators like `:=`,
        // `?=`, and `+=` are not mistaken for a rule colon.
        if let Some((key, op, value)) = parse_assignment(&line) {
            return self.handle_assignment(key, op, value);
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

        // Otherwise, throw error if line is not recognizable.
        Err(MakeError::new("Invalid line type.", self.context.clone()))
    }

    /// Apply a variable assignment with the semantics of its operator.
    fn handle_assignment(&mut self, key: &str, op: AssignOp, value: &str) -> Result<(), MakeError> {
        if key.trim().is_empty() {
            return Err(MakeError::new("Empty variable name.", self.context.clone()));
        }

        // make strips leading whitespace after the operator; the rest of the value is significant.
        let value = value.trim_start();
        let err = |e: String| MakeError::new(&e, self.context.clone());

        match op {
            // Recursive (`=`): store the raw value, expanded lazily at each use.
            AssignOp::Recursive => self.vars.set(key, value, true).map_err(err),

            // Simple (`:=` / `::=`): expand the value now and store the result.
            AssignOp::Simple => {
                let expanded = expand(value, &self.vars).map_err(err)?;
                self.vars.set(key, &expanded, false).map_err(err)
            }

            // Conditional (`?=`): set (recursively) only if not already defined.
            AssignOp::Conditional => {
                if self.vars.is_set(key) {
                    return Ok(());
                }
                self.vars.set(key, value, true).map_err(err)
            }

            // Append (`+=`): if undefined, act like `=`; otherwise append preserving the target's
            // flavor (simple variables expand the addition now, recursive ones keep it raw).
            AssignOp::Append => {
                if !self.vars.is_set(key) {
                    return self.vars.set(key, value, true).map_err(err);
                }

                let existing = self.vars.get(key);
                let recursive = existing.recursive;
                let existing_value = existing.value.clone();

                let addition = if recursive {
                    value.to_string()
                } else {
                    expand(value, &self.vars).map_err(err)?
                };
                let new_value = if existing_value.is_empty() {
                    addition
                } else {
                    format!("{existing_value} {addition}")
                };

                self.vars.set(key, &new_value, recursive).map_err(err)
            }

            // Shell (`!=`): run the (expanded) value through the shell; store stdout with trailing
            // newlines removed and remaining newlines turned into spaces. GNU make makes the result
            // a recursively-expanded variable, so `$` in the output is re-expanded on use.
            AssignOp::Shell => {
                let cmd = expand(value, &self.vars).map_err(err)?;
                let shell = self.vars.get("SHELL").value.clone();
                let shell_flags = self
                    .vars
                    .get(".SHELLFLAGS")
                    .value
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();

                let output = Command::new(&shell)
                    .args(&shell_flags)
                    .arg(&cmd)
                    .output()
                    .map_err(|e| MakeError::new(&e.to_string(), self.context.clone()))?;

                let mut result = String::from_utf8_lossy(&output.stdout).into_owned();
                while result.ends_with('\n') || result.ends_with('\r') {
                    result.pop();
                }
                let result = result.replace(['\n', '\r'], " ");

                self.vars.set(key, &result, true).map_err(err)
            }
        }
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

/// State for a logical line being assembled from continued physical lines.
struct PendingLine {
    /// Index of the first physical line (for context/error reporting).
    start_index: usize,
    /// Whether the logical line is a recipe (the first physical line starts with the recipe prefix).
    is_recipe: bool,
    /// The recipe prefix in effect when this logical line began.
    recipe_prefix: String,
    /// The accumulated logical line so far.
    accum: String,
}

/// Whether `line` ends with an odd number of backslashes, i.e. it continues onto the next line (an
/// even count means the final backslash is itself escaped, so the line does not continue).
fn ends_with_odd_backslash(line: &str) -> bool {
    line.bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}

/// Remove the single trailing backslash that marks a line continuation.
fn strip_one_trailing_backslash(line: &str) -> String {
    let mut s = line.to_string();
    s.pop();
    s
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

/// The flavor of a variable assignment operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignOp {
    /// `=` — recursively expanded (lazy): store the raw value, expand at each use.
    Recursive,
    /// `:=` / `::=` — simply expanded: expand the value once, now.
    Simple,
    /// `?=` — set only if the variable is not already defined.
    Conditional,
    /// `+=` — append, preserving the target variable's flavor.
    Append,
    /// `!=` — run the value as a shell command and store its output.
    Shell,
}

/// Detect a top-level variable assignment in `line`, returning `(key, op, value)`, or `None` if the
/// line is not an assignment (e.g. it is a rule). The operator is found before any rule colon: a `:`
/// that is not part of `:=`/`::=` means the line is a rule. Text inside `$(...)`/`${...}` is skipped
/// so operators there are not mistaken for the assignment. `key`/`value` are not trimmed here.
fn parse_assignment(line: &str) -> Option<(&str, AssignOp, &str)> {
    let bytes = line.as_bytes();
    let mut depth: usize = 0;

    for i in 0..bytes.len() {
        let c = bytes[i];

        // Inside a `$(...)`/`${...}` reference we only balance delimiters; operators there belong to
        // the reference, not the assignment.
        if depth > 0 {
            match c {
                b'(' | b'{' => depth += 1,
                b')' | b'}' => depth -= 1,
                _ => {}
            }
            continue;
        }

        match c {
            // Only `$(` / `${` open a reference; a bare `(`/`{` is a literal part of the name.
            b'(' | b'{' if i > 0 && bytes[i - 1] == b'$' => depth = 1,
            b':' => {
                // `::=` and `:=` are assignments; any other `:` starts a rule.
                if line[i..].starts_with("::=") {
                    return Some((&line[..i], AssignOp::Simple, &line[i + 3..]));
                }
                if line[i..].starts_with(":=") {
                    return Some((&line[..i], AssignOp::Simple, &line[i + 2..]));
                }
                return None;
            }
            b'=' => {
                // `+=`, `?=`, `!=` require the modifier char to immediately precede `=`.
                let (op, key_end) = match (i > 0).then(|| bytes[i - 1]) {
                    Some(b'+') => (AssignOp::Append, i - 1),
                    Some(b'?') => (AssignOp::Conditional, i - 1),
                    Some(b'!') => (AssignOp::Shell, i - 1),
                    _ => (AssignOp::Recursive, i),
                };
                return Some((&line[..key_end], op, &line[i + 1..]));
            }
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_assignment_operators() {
        assert_eq!(
            parse_assignment("X = y"),
            Some(("X ", AssignOp::Recursive, " y"))
        );
        assert_eq!(
            parse_assignment("X := y"),
            Some(("X ", AssignOp::Simple, " y"))
        );
        assert_eq!(
            parse_assignment("X ::= y"),
            Some(("X ", AssignOp::Simple, " y"))
        );
        assert_eq!(
            parse_assignment("X ?= y"),
            Some(("X ", AssignOp::Conditional, " y"))
        );
        assert_eq!(
            parse_assignment("X += y"),
            Some(("X ", AssignOp::Append, " y"))
        );
        assert_eq!(
            parse_assignment("X != y"),
            Some(("X ", AssignOp::Shell, " y"))
        );
    }

    #[test]
    fn test_parse_assignment_no_spaces() {
        assert_eq!(parse_assignment("X:=y"), Some(("X", AssignOp::Simple, "y")));
        assert_eq!(parse_assignment("X+=y"), Some(("X", AssignOp::Append, "y")));
    }

    #[test]
    fn test_parse_assignment_rules_return_none() {
        // A `:` that is not part of `:=`/`::=` means the line is a rule, so even a later `=` (in a
        // prerequisite) does not make it an assignment.
        assert_eq!(parse_assignment("target: dep"), None);
        assert_eq!(parse_assignment("target:: dep"), None);
        assert_eq!(parse_assignment("all: CFLAGS=-g"), None);
        assert_eq!(parse_assignment("just some words"), None);
    }

    #[test]
    fn test_parse_assignment_first_operator_wins() {
        // Later `:`/`=` are part of the value.
        assert_eq!(
            parse_assignment("X = a:b=c"),
            Some(("X ", AssignOp::Recursive, " a:b=c"))
        );
    }

    #[test]
    fn test_parse_assignment_skips_var_references() {
        // A `:`/`=` inside `$(...)` is not the operator.
        assert_eq!(
            parse_assignment("X = $(patsubst %.c,%.o,$(SRC))"),
            Some(("X ", AssignOp::Recursive, " $(patsubst %.c,%.o,$(SRC))")),
        );
        // A `:` inside a `$(...)` in the key is skipped; the real `:=` is still found.
        assert_eq!(
            parse_assignment("$(subst a:b,c,d)x := z"),
            Some(("$(subst a:b,c,d)x ", AssignOp::Simple, " z")),
        );
        // ...but the rule colon after a `$(...)` target is still detected.
        assert_eq!(parse_assignment("$(OBJ): $(SRC)"), None);
    }

    #[test]
    fn test_parse_assignment_literal_parens_in_name() {
        // Balanced `(`/`{` not preceded by `$` are literal name chars, not a reference.
        assert_eq!(
            parse_assignment("foo(bar)=value"),
            Some(("foo(bar)", AssignOp::Recursive, "value")),
        );
    }
}
