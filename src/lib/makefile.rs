//! The core logic for parsing and executing makefiles.

pub mod logical_line;
pub mod opts;
pub mod rule_map;

use opts::Opts;

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, fs::File};

use crate::error::MakeError;
use crate::expand::{expand, expand_collect};
use crate::logger::{Logger, context::Context};
use crate::vars::{Origin, Vars};

use rule_map::{Rule, RuleMap};

const COMMENT_INDICATOR: char = '#';

/// Data collected from special dot-targets (`.PHONY`, `.SILENT`, ...).
#[derive(Debug, Default)]
pub struct SpecialTargets {
    /// Targets that are always considered out of date (their recipes always run).
    phony: HashSet<String>,
    /// Targets whose recipes are not echoed.
    silent: HashSet<String>,
    /// Whether every recipe is silenced (`.SILENT` with no prerequisites).
    silent_all: bool,
    /// Targets whose recipe errors are ignored.
    ignore: HashSet<String>,
    /// Whether errors are ignored for every recipe (`.IGNORE` with no prerequisites).
    ignore_all: bool,
    /// Recipe for `.DEFAULT`, used for targets that have no rule of their own.
    default_recipe: Vec<String>,
    /// Targets exempt from delete-on-error/interrupt (`.PRECIOUS`).
    precious: HashSet<String>,
    /// Whether a target modified by a failing recipe is deleted (`.DELETE_ON_ERROR:`).
    delete_on_error: bool,
}

/// State for a `define ... endef` multi-line variable being captured.
#[derive(Debug)]
struct DefineCapture {
    /// Variable name being defined.
    name: String,
    /// Assignment flavor from the `define` line (`define X :=`, etc.); default is recursive.
    op: AssignOp,
    /// `File`, or `Override` for `override define`.
    origin: Origin,
    /// Body lines collected so far (verbatim).
    body: Vec<String>,
    /// Nesting depth of inner `define`/`endef` pairs within the body.
    depth: usize,
}

/// One level of `ifeq`/`ifdef`/... nesting.
#[derive(Debug)]
struct CondState {
    /// Whether the currently-selected branch is active (already accounts for enclosing levels).
    active: bool,
    /// Whether any branch at this level has been active (so `else` knows not to re-activate).
    taken: bool,
    /// Whether the enclosing context was active when this conditional began.
    parent_active: bool,
}

/// The primary interface for reading, parsing, and executing a makefile.
#[derive(Debug)]
pub struct Makefile<L: Logger> {
    pub opts: Opts,
    pub logger: Box<L>,

    rule_map: RuleMap,
    default_target: Option<String>,
    special: SpecialTargets,

    /// Names of variables marked for `export` into recipe child environments.
    exported: HashSet<String>,
    /// Names marked `unexport` — removed from recipe child environments (even if inherited).
    unexported: HashSet<String>,
    /// A bare `export` / `.EXPORT_ALL_VARIABLES:` exports every file/environment variable with a
    /// valid identifier name (minus any `unexport`ed).
    export_all: bool,
    /// `vpath PATTERN DIRS` directives: directories to search for files matching each pattern.
    vpaths: Vec<(String, Vec<String>)>,
    /// Known suffixes (from `.SUFFIXES`), used to recognize suffix rules like `.c.o:`.
    suffixes: Vec<String>,

    // Parser state.
    pub vars: Vars,
    current_rule: Option<Rule>,
    context: Context,
    cond_stack: Vec<CondState>,
    defining: Option<DefineCapture>,
    include_depth: usize,
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
            special: SpecialTargets::default(),
            exported: HashSet::new(),
            unexported: HashSet::new(),
            export_all: false,
            vpaths: Vec::new(),
            suffixes: crate::vars::DEFAULT_SUFFIXES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            vars,
            current_rule: None,
            context: path.clone().into(),
            cond_stack: Vec::new(),
            defining: None,
            include_depth: 0,
        };

        // Open the makefile and run it through the parser.
        let file = File::open(&path).map_err(|e| {
            MakeError::new(&format!("Could not read makefile ({}).", e), path.into())
        })?;
        makefile.parse(BufReader::new(file))?;

        // Built-in implicit rules are appended after the user's rules so they take lower priority.
        makefile.rule_map.register_builtins();

        Ok(makefile)
    }

    /// Iterate over the makefile's lines, call `parse_line` to handle the actual parsing logic, and
    /// manage context.
    fn parse<R: BufRead>(&mut self, input: R) -> Result<(), MakeError> {
        self.current_rule = None;

        self.parse_reader(input)?;

        // Always push two blank lines at the end to terminate trailing rules, even if the last rule
        // contained a trailing backslash.
        self.parse_line("".to_string())?;
        self.parse_line("".to_string())?;

        if !self.cond_stack.is_empty() {
            return Err(MakeError::new("Missing `endif`.", self.context.clone()));
        }

        if self.defining.is_some() {
            return Err(MakeError::new("Missing `endef`.", self.context.clone()));
        }

        Ok(())
    }

    /// Read `input`, join continued physical lines into logical lines, and hand each to
    /// `parse_line`. Shared by the top-level parse and by `include` handling (which reuses the
    /// current parser state).
    fn parse_reader<R: BufRead>(&mut self, input: R) -> Result<(), MakeError> {
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

        Ok(())
    }

    /// The line parser is where the "meat" of the parsing occurs. This is responsible for
    /// extracting rules from the physical lines of the makefile input, properly handling escaped
    /// newlines and semicolons, and also managing state, such as variable assignments and
    /// annotating when the parser moves in-to and out-of a rule definition.
    fn parse_line(&mut self, line: String) -> Result<(), MakeError> {
        // While capturing a `define` body, everything is verbatim until the matching `endef`.
        if self.defining.is_some() {
            return self.capture_define_line(line);
        }

        let recipe_prefix = self.vars.get(".RECIPEPREFIX").value.to_string();

        // A line beginning with the recipe prefix is a recipe (or a "recipe without rule" error),
        // never a directive. Any other line is checked for conditional directives first — even
        // inside an inactive branch — so the conditional stack stays balanced. Comments are
        // stripped here so directives like `ifeq (a,b) # note` parse correctly.
        if !line.starts_with(&recipe_prefix) {
            let stripped = strip_inline_comment(&line);
            let trimmed = stripped.trim();
            if directive_keyword(trimmed).is_some() {
                return self.handle_conditional(trimmed);
            }
            if let Some((kw, rest)) = bsd_cond_keyword(trimmed) {
                return self.handle_bsd_conditional(kw, rest);
            }
        }

        // Skip everything else while inside an inactive conditional branch.
        if !self.cond_active() {
            return Ok(());
        }

        // Handle recipe lines.
        let recipe_prefix = &recipe_prefix;
        if line.starts_with(recipe_prefix) {
            // If line starts with the recipe prefix, then push it to the current rule.
            match &mut self.current_rule {
                None => return Err(MakeError::new("recipe without rule", self.context.clone())),
                Some(r) => {
                    // Strip the recipe prefix and store the command RAW; recipes are expanded at
                    // execution time (so later variable definitions and automatic variables like
                    // `$@` resolve). A blank recipe line is not stored, but its presence still marks
                    // the rule as having a recipe section (gmake treats `target:;` and a bare blank
                    // recipe line as "has commands").
                    let cmd = line
                        .strip_prefix(recipe_prefix)
                        .expect("line known to start with a recipe prefix");

                    r.has_recipe = true;
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
        if let Some(mut rule) = self.current_rule.take() {
            // Collect any special dot-target directives from this rule (this may update the suffix
            // list via `.SUFFIXES`, which the suffix-rule rewrite below then uses). A static pattern
            // rule is never a special/suffix rule, so skip both for it.
            if rule.target_pattern.is_none() {
                self.collect_special_targets(&rule);

                // Rewrite a suffix rule (e.g. `.c.o:`) into the equivalent pattern rule (`%.o: %.c`).
                self.rewrite_suffix_rule(&mut rule);
            }

            // If there is no default target, see if we can assign one. A dot-target (`.PHONY`) or a
            // `%` pattern rule is never the default goal, matching GNU make.
            if self.default_target.is_none() {
                for target in rule.targets.iter() {
                    if self.default_target.is_none()
                        && !target.starts_with('.')
                        && !target.contains('%')
                    {
                        self.default_target = Some(target.clone());
                    }
                }
            }

            // A static pattern rule expands into one concrete rule per listed target; otherwise add
            // the rule as-is.
            match rule.target_pattern.take() {
                Some(target_pattern) => self.insert_static_pattern(rule, &target_pattern)?,
                None => self.rule_map.insert(rule, &*self.logger)?,
            }
        }

        // Ignore pure comments and blank lines.
        let trimmed_line = line.trim();
        if trimmed_line.starts_with(COMMENT_INDICATOR) || trimmed_line.is_empty() {
            return Ok(());
        }

        // Handle `include` / `-include` / `sinclude` directives.
        if let Some((optional, names)) = parse_include(trimmed_line) {
            return self.handle_include(optional, names);
        }

        // Handle BSD `.include "file"` / `.include <file>` (and the optional `.-include` /
        // `.sinclude` variants). System-header form (`<...>`) is treated like the quoted form. The
        // name is a single file (which may contain spaces), so it bypasses the GNU list tokenizer.
        if let Some((optional, name)) = parse_bsd_include(trimmed_line) {
            let expanded =
                expand(name, &self.vars).map_err(|e| MakeError::new(&e, self.context.clone()))?;
            return self.include_file(expanded.trim(), optional);
        }

        // Handle `export` / `unexport` directives.
        if let Some((export, rest)) = parse_export(trimmed_line) {
            return self.handle_export(export, rest);
        }

        // Handle `[override] undefine NAME`.
        if let Some((override_, name)) = parse_undefine(trimmed_line) {
            return self.handle_undefine(override_, name);
        }

        // An optional leading `override` (with `Override` precedence) applies to a `define` or an
        // assignment. Strip it so both forms below can handle it.
        let (origin, directive) = match trimmed_line.strip_prefix("override") {
            Some(rest) if rest.starts_with([' ', '\t']) => (Origin::Override, rest.trim_start()),
            _ => (Origin::File, trimmed_line),
        };

        // Handle `[override] define NAME [op]` — begin capturing a multi-line variable.
        if let Some(rest) = parse_define(directive) {
            let (name, op) = match parse_assignment(rest) {
                Some((key, op, _)) => (key.trim().to_string(), op),
                None => (rest.trim().to_string(), AssignOp::Recursive),
            };
            self.defining = Some(DefineCapture {
                name,
                op,
                origin,
                body: Vec::new(),
                depth: 0,
            });
            return Ok(());
        }

        // Handle `override VAR op value` — an assignment with override precedence (it beats a
        // command-line variable). Must run before plain assignment detection, which would otherwise
        // read `override VAR` as the key.
        if origin == Origin::Override
            && let Some((key, op, value)) = parse_assignment(directive)
        {
            return self.assign(key, op, value.trim_start(), Origin::Override);
        }

        // Handle `vpath` directives.
        if let Some(rest) = parse_vpath(trimmed_line) {
            return self.handle_vpath(rest);
        }

        // Handle variable assignments. This must run before rule detection so operators like `:=`,
        // `?=`, and `+=` are not mistaken for a rule colon.
        if let Some((key, op, value)) = parse_assignment(&line) {
            return self.handle_assignment(key, op, value);
        }

        // A line that is only a function/variable reference (e.g. `$(info ...)`, `$(eval ...)`,
        // `$(error ...)`) is expanded for its side effects; if it expands to nothing, the line is a
        // no-op. This matches GNU make evaluating a bare function-call line, and runs before rule
        // detection so a `:` inside the reference is not mistaken for a rule colon.
        if trimmed_line.starts_with('$') {
            let expanded = self.expand_str(trimmed_line)?;
            if expanded.trim().is_empty() {
                return Ok(());
            }
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

            // A second colon in the dependency part introduces a static pattern rule
            // (`targets: target-pattern: prereq-patterns`). The `:` of a `:=`/`::=` target-specific
            // assignment is not a separator (`parse_assignment` recognizes those), so it is skipped.
            // GNU make requires the target pattern to contain `%` (fatal error otherwise); it is
            // checked after expansion, so a variable target pattern (`$(P): %.c`) is still detected.
            let target_pattern = if parse_assignment(deps).is_none()
                && let Some((tpat, ppat)) = deps.split_once(':')
            {
                let expanded = self.expand_str(tpat)?;
                if !expanded.contains('%') {
                    return Err(MakeError::new(
                        "target pattern contains no '%'.",
                        self.context.clone(),
                    ));
                }
                deps = ppat;
                Some(expanded.trim().to_string())
            } else {
                None
            };

            // A `|` separates normal prerequisites from order-only ones.
            let (normal_deps, order_deps) = match deps.split_once('|') {
                Some((n, o)) => (n, o),
                None => (deps, ""),
            };
            // Expand variables (evaluating any `$(eval ...)`), split into words, then glob each
            // word against the filesystem (a word with no match keeps its literal form, like GNU
            // make). Wildcards resolve at parse time, so only files present when the makefile is
            // read are matched. In a static pattern rule the `%`-bearing prerequisite *patterns* are
            // kept literal (the `%` is substituted per target at finalization); plain words still
            // glob like ordinary prerequisites.
            let targets = self.expand_str(targets)?;
            let normal_deps = self.expand_str(normal_deps)?;
            let order_deps = self.expand_str(order_deps)?;
            let deps_list = |s: &str| {
                if target_pattern.is_some() {
                    s.split_whitespace()
                        .flat_map(|w| {
                            if w.contains('%') {
                                vec![w.to_string()]
                            } else {
                                glob_words(std::iter::once(w.to_string()))
                            }
                        })
                        .collect()
                } else {
                    glob_words(s.split_whitespace().map(str::to_string))
                }
            };

            self.current_rule = Some(Rule {
                targets: glob_words(targets.split_whitespace().map(str::to_string)),
                prerequisites: deps_list(&normal_deps),
                order_only: deps_list(&order_deps),
                recipe: vec![],
                has_recipe: false,
                stem: String::new(),
                target_pattern,
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

    /// Apply a variable assignment with the semantics of its operator, stripping the leading
    /// whitespace that follows the operator (`FOO =   value`).
    fn handle_assignment(&mut self, key: &str, op: AssignOp, value: &str) -> Result<(), MakeError> {
        self.assign(key, op, value.trim_start(), Origin::File)
    }

    /// Apply an assignment with `value` used verbatim (no leading-whitespace trimming), recording
    /// `origin` (`File` for a normal assignment, `Override` for the `override` directive). Used for
    /// `define` bodies too, where leading whitespace is significant.
    fn assign(
        &mut self,
        key: &str,
        op: AssignOp,
        value: &str,
        origin: Origin,
    ) -> Result<(), MakeError> {
        if key.trim().is_empty() {
            return Err(MakeError::new("Empty variable name.", self.context.clone()));
        }

        // Capture a cloned context (not `self`) so the closure does not borrow `self`, leaving
        // `self.expand_str` (which needs `&mut self` to process `$(eval ...)`) free to run.
        let context = self.context.clone();
        let err = |e: String| MakeError::new(&e, context.clone());

        match op {
            // Recursive (`=`): store the raw value, expanded lazily at each use.
            AssignOp::Recursive => self.vars.set_origin(key, value, true, origin).map_err(err),

            // Simple (`:=` / `::=`): expand the value now and store the result.
            AssignOp::Simple => {
                let expanded = self.expand_str(value)?;
                self.vars
                    .set_origin(key, &expanded, false, origin)
                    .map_err(err)
            }

            // Conditional (`?=`): set (recursively) only if not already defined.
            AssignOp::Conditional => {
                if self.vars.is_set(key) {
                    return Ok(());
                }
                self.vars.set_origin(key, value, true, origin).map_err(err)
            }

            // Append (`+=`): if undefined, act like `=`; otherwise append preserving the target's
            // flavor (simple variables expand the addition now, recursive ones keep it raw).
            AssignOp::Append => {
                if !self.vars.is_set(key) {
                    return self.vars.set_origin(key, value, true, origin).map_err(err);
                }

                let existing = self.vars.get(key);
                let recursive = existing.recursive;
                let existing_value = existing.value.clone();

                let addition = if recursive {
                    value.to_string()
                } else {
                    self.expand_str(value)?
                };
                let new_value = if existing_value.is_empty() {
                    addition
                } else {
                    format!("{existing_value} {addition}")
                };

                self.vars
                    .set_origin(key, &new_value, recursive, origin)
                    .map_err(err)
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
                    // Capture stdout only; let the command's stderr reach the terminal.
                    .stderr(std::process::Stdio::inherit())
                    .output()
                    .map_err(|e| MakeError::new(&e.to_string(), self.context.clone()))?;

                // `!=` removes only the final newline (and a preceding CR), then turns the
                // remaining newlines into spaces — unlike `$(shell)`, which strips all trailing
                // newlines.
                let mut result = String::from_utf8_lossy(&output.stdout).into_owned();
                if result.ends_with('\n') {
                    result.pop();
                    if result.ends_with('\r') {
                        result.pop();
                    }
                }
                let result = result.replace("\r\n", " ").replace('\n', " ");

                self.vars
                    .set_origin(key, &result, true, origin)
                    .map_err(err)
            }
        }
    }

    /// Expand `s`, then parse any text produced by `$(eval ...)` as makefile syntax. The eval text
    /// is processed *after* expansion finishes (once the borrow of the variables is released), so a
    /// variable it defines is visible on later lines but not within the same expression.
    fn expand_str(&mut self, s: &str) -> Result<String, MakeError> {
        let (result, evals) =
            expand_collect(s, &self.vars).map_err(|e| MakeError::new(&e, self.context.clone()))?;
        for text in evals {
            self.eval_makefile_text(&text)?;
        }
        Ok(result)
    }

    /// Parse `text` (the already-expanded argument of a `$(eval ...)`) as makefile source, then
    /// finalize any pending rule (two blank lines, as end-of-file does). The parse context is saved
    /// and restored so the caller's line reporting is unaffected.
    fn eval_makefile_text(&mut self, text: &str) -> Result<(), MakeError> {
        let saved = self.context.clone();
        let result = self
            .parse_reader(std::io::Cursor::new(text.as_bytes()))
            .and_then(|()| self.parse_line(String::new()))
            .and_then(|()| self.parse_line(String::new()));
        self.context = saved;
        result
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

        self.rule_map.execute(self, &targets)
    }

    /// Whether the parser is currently in an active (non-skipped) conditional branch.
    fn cond_active(&self) -> bool {
        self.cond_stack.last().is_none_or(|c| c.active)
    }

    /// Handle a conditional directive line (`ifeq`/`ifneq`/`ifdef`/`ifndef`/`else`/`endif`).
    fn handle_conditional(&mut self, trimmed: &str) -> Result<(), MakeError> {
        let kw = directive_keyword(trimmed).expect("caller checked this is a directive");
        let rest = trimmed[kw.len()..].trim();

        match kw {
            "ifeq" | "ifneq" | "ifdef" | "ifndef" => {
                let parent_active = self.cond_active();
                // Only evaluate the condition when the enclosing context is active.
                let cond = parent_active && self.eval_condition(kw, rest)?;
                self.cond_stack.push(CondState {
                    active: cond,
                    taken: cond,
                    parent_active,
                });
            }
            "else" => {
                let (parent_active, already_taken) = match self.cond_stack.last() {
                    Some(s) => (s.parent_active, s.taken),
                    None => {
                        return Err(MakeError::new(
                            "`else` without conditional.",
                            self.context.clone(),
                        ));
                    }
                };

                // `else` may be followed by another condition (`else ifeq (...)`), but only by an
                // `if*` directive — not another `else`/`endif`.
                let activate = if rest.is_empty() {
                    parent_active && !already_taken
                } else {
                    let follow = directive_keyword(rest)
                        .filter(|k| matches!(*k, "ifeq" | "ifneq" | "ifdef" | "ifndef"));
                    let Some(follow) = follow else {
                        return Err(MakeError::new(
                            "Extraneous text after `else`.",
                            self.context.clone(),
                        ));
                    };
                    let can_take = parent_active && !already_taken;
                    can_take && self.eval_condition(follow, rest[follow.len()..].trim())?
                };

                let state = self.cond_stack.last_mut().unwrap();
                state.active = activate;
                state.taken = state.taken || activate;
            }
            "endif" => {
                if self.cond_stack.pop().is_none() {
                    return Err(MakeError::new(
                        "`endif` without conditional.",
                        self.context.clone(),
                    ));
                }
            }
            _ => unreachable!("directive_keyword only returns known directives"),
        }

        Ok(())
    }

    /// Evaluate an `ifeq`/`ifneq`/`ifdef`/`ifndef` condition.
    fn eval_condition(&self, kw: &str, rest: &str) -> Result<bool, MakeError> {
        let err = |e: String| MakeError::new(&e, self.context.clone());
        match kw {
            "ifeq" | "ifneq" => {
                let (a, b) = parse_ifeq_args(rest).ok_or_else(|| {
                    MakeError::new("Malformed `ifeq`/`ifneq` arguments.", self.context.clone())
                })?;
                let equal =
                    expand(&a, &self.vars).map_err(err)? == expand(&b, &self.vars).map_err(err)?;
                Ok(if kw == "ifeq" { equal } else { !equal })
            }
            "ifdef" | "ifndef" => {
                // The name is expanded; the variable is "defined" if its (unexpanded) value is
                // non-empty, matching GNU make.
                let name = expand(rest, &self.vars).map_err(err)?;
                let defined = !self.vars.get(name.trim()).value.is_empty();
                Ok(if kw == "ifdef" { defined } else { !defined })
            }
            _ => unreachable!(),
        }
    }

    /// Push a conditional level whose branch is active iff `cond` (which already accounts for the
    /// enclosing context).
    fn cond_push(&mut self, cond: bool) {
        let parent_active = self.cond_active();
        self.cond_stack.push(CondState {
            active: cond,
            taken: cond,
            parent_active,
        });
    }

    /// Switch to an `else`/`elif` branch. `follow` is the follow-on condition (`None` for a plain
    /// `else`, always taken if no earlier branch was).
    fn cond_else(&mut self, follow: Option<bool>) -> Result<(), MakeError> {
        let (parent_active, taken) = match self.cond_stack.last() {
            Some(s) => (s.parent_active, s.taken),
            None => {
                return Err(MakeError::new(
                    "`else`/`elif` without conditional.",
                    self.context.clone(),
                ));
            }
        };
        let activate = parent_active && !taken && follow.unwrap_or(true);
        let state = self.cond_stack.last_mut().unwrap();
        state.active = activate;
        state.taken = state.taken || activate;
        Ok(())
    }

    /// Close the innermost conditional level.
    fn cond_endif(&mut self) -> Result<(), MakeError> {
        if self.cond_stack.pop().is_none() {
            return Err(MakeError::new(
                "`endif` without conditional.",
                self.context.clone(),
            ));
        }
        Ok(())
    }

    /// Handle a BSD conditional directive (`.if`/`.ifdef`/`.ifndef`/`.elif`/`.else`/`.endif`, and
    /// the `.elifdef`/`.elifndef` variants).
    fn handle_bsd_conditional(&mut self, kw: &str, rest: &str) -> Result<(), MakeError> {
        match kw {
            ".if" => {
                let cond = self.cond_active() && self.eval_bsd_expr(rest)?;
                self.cond_push(cond);
            }
            ".ifdef" => {
                let cond = self.cond_active() && self.bsd_defined(rest)?;
                self.cond_push(cond);
            }
            ".ifndef" => {
                let cond = self.cond_active() && !self.bsd_defined(rest)?;
                self.cond_push(cond);
            }
            ".elif" => {
                let follow = self.eval_bsd_expr(rest)?;
                self.cond_else(Some(follow))?;
            }
            ".elifdef" => {
                let follow = self.bsd_defined(rest)?;
                self.cond_else(Some(follow))?;
            }
            ".elifndef" => {
                let follow = !self.bsd_defined(rest)?;
                self.cond_else(Some(follow))?;
            }
            ".else" => self.cond_else(None)?,
            ".endif" => self.cond_endif()?,
            _ => unreachable!("bsd_cond_keyword only returns known directives"),
        }
        Ok(())
    }

    /// Whether the (expanded) variable named by `rest` is defined (BSD `defined()`/`.ifdef`).
    fn bsd_defined(&self, rest: &str) -> Result<bool, MakeError> {
        let name =
            expand(rest, &self.vars).map_err(|e| MakeError::new(&e, self.context.clone()))?;
        Ok(self.vars.is_set(name.trim()))
    }

    /// Evaluate a BSD `.if` expression. A recursive-descent evaluator with `||` (lowest), then
    /// `&&`, then a primary (`!`, parentheses, `defined()`/`empty()`, comparisons, bare value).
    fn eval_bsd_expr(&self, expr: &str) -> Result<bool, MakeError> {
        self.eval_bsd_or(expr.trim())
    }

    /// `||` level (lowest precedence), short-circuiting.
    fn eval_bsd_or(&self, expr: &str) -> Result<bool, MakeError> {
        if let Some(parts) = split_logical(expr, "||") {
            for part in parts {
                if self.eval_bsd_and(part.trim())? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        self.eval_bsd_and(expr)
    }

    /// `&&` level, short-circuiting.
    fn eval_bsd_and(&self, expr: &str) -> Result<bool, MakeError> {
        if let Some(parts) = split_logical(expr, "&&") {
            for part in parts {
                if !self.eval_bsd_primary(part.trim())? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        self.eval_bsd_primary(expr)
    }

    /// A primary term: `!expr`, `(expr)`, `defined(x)`/`empty(x)`, a comparison, or a bare value
    /// (true when it expands to a non-empty, non-zero string).
    fn eval_bsd_primary(&self, expr: &str) -> Result<bool, MakeError> {
        let expr = expr.trim();
        let err = |e: String| MakeError::new(&e, self.context.clone());

        if let Some(inner) = strip_full_parens(expr) {
            return self.eval_bsd_or(inner.trim());
        }
        if let Some(rest) = expr.strip_prefix('!') {
            return Ok(!self.eval_bsd_primary(rest)?);
        }
        if let Some(inner) = expr
            .strip_prefix("defined(")
            .and_then(|s| s.strip_suffix(')'))
        {
            return self.bsd_defined(inner);
        }
        if let Some(inner) = expr
            .strip_prefix("empty(")
            .and_then(|s| s.strip_suffix(')'))
        {
            // `empty(VAR)` is true when the variable expands to nothing.
            let value = expand(&format!("${{{}}}", inner.trim()), &self.vars).map_err(err)?;
            return Ok(value.is_empty());
        }

        // Comparison operators. Two-character forms are tried before one-character ones so `<=` is
        // not mistaken for `<`.
        for op in ["==", "!=", "<=", ">=", "<", ">"] {
            if let Some((lhs, rhs)) = expr.split_once(op) {
                return self.eval_bsd_compare(lhs, rhs, op);
            }
        }

        // A bare expression is true when it expands to a non-empty, non-zero value.
        let value = expand(expr, &self.vars).map_err(err)?;
        let value = value.trim();
        Ok(!value.is_empty() && value != "0")
    }

    /// Evaluate a BSD comparison. `==`/`!=` compare numerically when both operands are unquoted
    /// numbers, otherwise as strings; the relational operators are always numeric (matching bmake).
    fn eval_bsd_compare(&self, lhs: &str, rhs: &str, op: &str) -> Result<bool, MakeError> {
        let (lval, lquoted) = self.bsd_operand(lhs)?;
        let (rval, rquoted) = self.bsd_operand(rhs)?;
        let lnum = (!lquoted).then(|| bsd_number(&lval)).flatten();
        let rnum = (!rquoted).then(|| bsd_number(&rval)).flatten();

        if op == "==" || op == "!=" {
            let equal = match (lnum, rnum) {
                (Some(a), Some(b)) => a == b,
                _ => lval == rval,
            };
            return Ok(if op == "==" { equal } else { !equal });
        }

        let (a, b) = match (lnum, rnum) {
            (Some(a), Some(b)) => (a, b),
            _ => {
                return Err(MakeError::new(
                    &format!("Comparison `{op}` requires numeric operands."),
                    self.context.clone(),
                ));
            }
        };
        Ok(match op {
            "<" => a < b,
            "<=" => a <= b,
            ">" => a > b,
            ">=" => a >= b,
            _ => unreachable!("eval_bsd_expr only passes known comparison operators"),
        })
    }

    /// Expand a BSD comparison operand, returning its value (quotes stripped) and whether it was
    /// quoted (a quoted operand is always compared as a string).
    fn bsd_operand(&self, operand: &str) -> Result<(String, bool), MakeError> {
        let expanded = expand(operand.trim(), &self.vars)
            .map_err(|e| MakeError::new(&e, self.context.clone()))?;
        let trimmed = expanded.trim();
        if let Some(inner) = trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return Ok((inner.to_string(), true));
        }
        Ok((trimmed.to_string(), false))
    }

    /// Handle a GNU `include`/`-include`/`sinclude` directive: expand the (whitespace-separated)
    /// name list and parse each file. `include` errors on a missing file; the others ignore it.
    fn handle_include(&mut self, optional: bool, names: &str) -> Result<(), MakeError> {
        let expanded =
            expand(names, &self.vars).map_err(|e| MakeError::new(&e, self.context.clone()))?;

        // Glob each operand; a pattern with no match keeps its literal form (so `include` still
        // errors on it, while `-include`/`sinclude` stay silent), matching GNU make.
        for name in glob_words(expanded.split_whitespace().map(str::to_string)) {
            self.include_file(&name, optional)?;
        }

        Ok(())
    }

    /// Parse a single named file into the current parser state. A missing file is an error unless
    /// `optional`. The name is used verbatim (already expanded and, for BSD `.include`, unquoted),
    /// so filenames may contain spaces.
    fn include_file(&mut self, name: &str, optional: bool) -> Result<(), MakeError> {
        const INCLUDE_DEPTH_LIMIT: usize = 200;
        if self.include_depth >= INCLUDE_DEPTH_LIMIT {
            return Err(MakeError::new(
                "`include` nested too deeply (recursive include?).",
                self.context.clone(),
            ));
        }

        let file = match File::open(name) {
            Ok(file) => file,
            Err(e) => {
                if optional {
                    return Ok(());
                }
                return Err(MakeError::new(
                    &format!("Could not include {name} ({e})."),
                    self.context.clone(),
                ));
            }
        };

        // Parse the included file with its own context, restoring ours afterwards.
        let cond_depth = self.cond_stack.len();
        let saved = self.context.clone();
        self.context = PathBuf::from(name).into();
        self.include_depth += 1;
        let result = self.parse_reader(BufReader::new(file));
        self.include_depth -= 1;

        // Conditionals may not cross a file boundary, so the stack must return to its prior depth.
        // Report against the included file before restoring the parent's context.
        if result.is_ok() && self.cond_stack.len() != cond_depth {
            let err = MakeError::new(
                "Unbalanced conditional (`ifeq`/`endif`) in included file.",
                self.context.clone(),
            );
            self.context = saved;
            return Err(err);
        }

        self.context = saved;
        result
    }

    /// Append a line to the in-progress `define` body, or finish it on the matching `endef`. Inner
    /// `define`/`endef` pairs nest and are part of the body. A recipe-prefixed (tab) line is
    /// literal body content, never a `define`/`endef` directive.
    fn capture_define_line(&mut self, line: String) -> Result<(), MakeError> {
        let recipe_prefix = self.vars.get(".RECIPEPREFIX").value.to_string();
        let is_directive = !line.starts_with(&recipe_prefix);
        let trimmed = line.trim();
        let starts_define = is_directive && parse_define(trimmed).is_some();
        let is_endef = is_directive && trimmed == "endef";

        // Finish the outermost define on `endef` at nesting depth 0.
        if is_endef && self.defining.as_ref().unwrap().depth == 0 {
            let capture = self.defining.take().unwrap();
            // The body is assigned verbatim (leading whitespace within it is significant).
            return self.assign(
                &capture.name,
                capture.op,
                &capture.body.join("\n"),
                capture.origin,
            );
        }

        let capture = self.defining.as_mut().unwrap();
        if starts_define {
            capture.depth += 1;
        } else if is_endef {
            capture.depth -= 1;
        }
        capture.body.push(line);
        Ok(())
    }

    /// Handle an `export`/`unexport` directive. `rest` may be empty, a list of variable names, or
    /// (for `export`) an assignment (`export NAME = value`), which both assigns and exports.
    fn handle_export(&mut self, export: bool, rest: &str) -> Result<(), MakeError> {
        let rest = rest.trim();

        // `export NAME = value` — perform the assignment, then export the (literal) name.
        if export && let Some((key, op, value)) = parse_assignment(rest) {
            self.handle_assignment(key, op, value)?;
            self.set_exported(key.trim(), true);
            return Ok(());
        }

        // A bare `export` / `unexport` (no names) toggles the export-all flag.
        if rest.is_empty() {
            self.export_all = export;
            return Ok(());
        }

        // Otherwise a (possibly computed) list of names.
        let expanded =
            expand(rest, &self.vars).map_err(|e| MakeError::new(&e, self.context.clone()))?;
        for name in expanded.split_whitespace() {
            self.set_exported(name, export);
        }
        Ok(())
    }

    /// Handle `[override] undefine NAME`: remove the variable. The whole (expanded) argument is a
    /// single name. Without `override`, a command-line/`override`-origin variable is left alone
    /// (matching assignment precedence). Removing a variable also drops it from recipe children.
    fn handle_undefine(&mut self, override_: bool, name: &str) -> Result<(), MakeError> {
        let name =
            expand(name, &self.vars).map_err(|e| MakeError::new(&e, self.context.clone()))?;
        let name = name.trim();
        if name.is_empty() {
            return Err(MakeError::new("Empty variable name.", self.context.clone()));
        }

        // Respect origin precedence: a plain `undefine` cannot remove a command-line or `override`
        // variable.
        if !override_
            && self.vars.is_set(name)
            && self.vars.get(name).origin.rank() > Origin::File.rank()
        {
            return Ok(());
        }

        self.vars.remove(name);
        // Drop it from recipe children too (it was exported, an env var, or neither).
        self.exported.remove(name);
        self.unexported.insert(name.to_string());
        Ok(())
    }

    /// Mark `name` as exported (or, if `export` is false, unexported), keeping the two sets
    /// disjoint.
    fn set_exported(&mut self, name: &str, export: bool) {
        if export {
            self.unexported.remove(name);
            self.exported.insert(name.to_string());
        } else {
            self.exported.remove(name);
            self.unexported.insert(name.to_string());
        }
    }

    /// The environment changes for a recipe child: variables to add (as `NAME=value`) and names to
    /// remove (so `unexport` also drops inherited environment variables).
    fn exported_env(&self, vars: &Vars) -> (Vec<(String, String)>, Vec<String>) {
        // Names to export: those named explicitly, plus (under export-all) every file/environment
        // variable with a valid identifier name. `unexport`ed names are excluded.
        let mut names: HashSet<&str> = self.exported.iter().map(String::as_str).collect();
        for (name, var) in vars.iter() {
            // Command-line variables are always exported; export-all also exports file/environment
            // variables with a valid identifier name.
            let cmdline = matches!(var.origin, Origin::CommandLine);
            let by_all = self.export_all
                && matches!(
                    var.origin,
                    Origin::File | Origin::Environment | Origin::Override
                )
                && is_env_identifier(name);
            if cmdline || by_all {
                names.insert(name);
            }
        }

        let adds = names
            .into_iter()
            .filter(|name| !self.unexported.contains(*name))
            .filter_map(|name| {
                expand(&vars.get(name).value, vars)
                    .ok()
                    .map(|value| (name.to_string(), value))
            })
            .collect();
        let removes = self.unexported.iter().cloned().collect();
        (adds, removes)
    }

    /// Handle a `vpath` directive: `vpath PATTERN DIRS` adds a search path, `vpath PATTERN` clears
    /// the paths for that pattern, and a bare `vpath` clears them all.
    fn handle_vpath(&mut self, rest: &str) -> Result<(), MakeError> {
        let expanded =
            expand(rest, &self.vars).map_err(|e| MakeError::new(&e, self.context.clone()))?;
        let rest = expanded.trim();

        if rest.is_empty() {
            self.vpaths.clear();
            return Ok(());
        }

        let mut parts = rest.splitn(2, char::is_whitespace);
        let pattern = parts.next().unwrap().to_string();
        match parts.next() {
            None => self.vpaths.retain(|(p, _)| p != &pattern),
            Some(dirs) => {
                let dirs = split_path_list(dirs);
                self.vpaths.push((pattern, dirs));
            }
        }
        Ok(())
    }

    /// Find `name` on disk, searching the current directory first and then the `VPATH` variable and
    /// any matching `vpath` directories. Returns the path where it was found, or `None`.
    fn find_file(&self, name: &str) -> Option<String> {
        if Path::new(name).exists() {
            return Some(name.to_string());
        }
        for dir in self.search_dirs(name) {
            let candidate = Path::new(&dir).join(name);
            if candidate.exists() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
        None
    }

    /// The directories to search for `name`. Pattern-specific `vpath` directives take precedence
    /// over the general `VPATH` variable (which is expanded, as it may be a recursive variable).
    fn search_dirs(&self, name: &str) -> Vec<String> {
        let mut dirs = Vec::new();
        for (pattern, pdirs) in &self.vpaths {
            if vpath_matches(pattern, name) {
                dirs.extend(pdirs.iter().cloned());
            }
        }
        let vpath = expand(&self.vars.get("VPATH").value, &self.vars).unwrap_or_default();
        dirs.extend(split_path_list(&vpath));
        dirs
    }

    /// If `rule` is a suffix rule (a single target made of known suffixes, with no prerequisites),
    /// rewrite it in place to the equivalent pattern rule so the pattern-rule machinery handles it.
    fn rewrite_suffix_rule(&self, rule: &mut Rule) {
        if rule.targets.len() != 1 || !rule.prerequisites.is_empty() || !rule.order_only.is_empty()
        {
            return;
        }
        if let Some((target_pattern, prereq_pattern)) =
            suffix_rule_patterns(&rule.targets[0], &self.suffixes)
        {
            rule.targets = vec![target_pattern];
            rule.prerequisites = vec![prereq_pattern];
        }
    }

    /// Expand a static pattern rule (`targets: target-pattern: prereq-patterns`) into one concrete
    /// rule per listed target: the target's stem (matched against the target pattern) substitutes
    /// the first `%` in each prerequisite pattern, and is exposed as `$*`. A target that does not
    /// match the pattern gets a non-fatal warning and a prereq-less rule with the whole target name
    /// as its stem (matching GNU make). A `%` in the target *list* is the "mixed implicit and ..."
    /// error/warning.
    fn insert_static_pattern(&mut self, rule: Rule, target_pattern: &str) -> Result<(), MakeError> {
        let (prefix, suffix) = target_pattern
            .split_once('%')
            .expect("static pattern target has a '%' (checked during parsing)");

        // A `%` in the target list mixes implicit and static rules; GNU make rejects an all-pattern
        // target list outright and deprecates a partial one (dropping the pattern targets).
        if !rule.targets.is_empty() && rule.targets.iter().all(|t| t.contains('%')) {
            return Err(MakeError::new(
                "mixed implicit and static pattern rules.",
                rule.context.clone(),
            ));
        }
        if rule.targets.iter().any(|t| t.contains('%')) {
            self.logger.warn(
                "mixed implicit and normal rules: deprecated syntax",
                Some(&rule.context),
            );
        }

        for target in rule.targets.iter().filter(|t| !t.contains('%')) {
            // A non-matching target still gets the recipe, but with no prerequisites; its stem
            // ($*) is the whole target name, as in GNU make.
            let (prerequisites, order_only, stem) = match pattern_stem(prefix, suffix, target) {
                Some(stem) => (
                    rule.prerequisites
                        .iter()
                        .map(|p| p.replacen('%', stem, 1))
                        .collect(),
                    rule.order_only
                        .iter()
                        .map(|p| p.replacen('%', stem, 1))
                        .collect(),
                    stem.to_string(),
                ),
                None => {
                    self.logger.warn(
                        &format!("target '{target}' doesn't match the target pattern"),
                        Some(&rule.context),
                    );
                    (vec![], vec![], target.clone())
                }
            };
            let concrete = Rule {
                targets: vec![target.clone()],
                prerequisites,
                order_only,
                recipe: rule.recipe.clone(),
                has_recipe: rule.has_recipe,
                stem,
                target_pattern: None,
                context: rule.context.clone(),
                double_colon: rule.double_colon,
            };
            self.rule_map.insert(concrete, &*self.logger)?;
        }
        Ok(())
    }

    /// Record `.PHONY`, `.SILENT`, `.IGNORE`, and `.DEFAULT` directives from a parsed rule.
    fn collect_special_targets(&mut self, rule: &Rule) {
        for target in &rule.targets {
            match target.as_str() {
                ".PHONY" => self
                    .special
                    .phony
                    .extend(rule.prerequisites.iter().cloned()),
                ".SILENT" => {
                    if rule.prerequisites.is_empty() {
                        self.special.silent_all = true;
                    } else {
                        self.special
                            .silent
                            .extend(rule.prerequisites.iter().cloned());
                    }
                }
                ".IGNORE" => {
                    if rule.prerequisites.is_empty() {
                        self.special.ignore_all = true;
                    } else {
                        self.special
                            .ignore
                            .extend(rule.prerequisites.iter().cloned());
                    }
                }
                ".DEFAULT" => self.special.default_recipe = rule.recipe.clone(),
                // `.EXPORT_ALL_VARIABLES:` is another spelling of a bare `export`.
                ".EXPORT_ALL_VARIABLES" => self.export_all = true,
                ".PRECIOUS" => self
                    .special
                    .precious
                    .extend(rule.prerequisites.iter().cloned()),
                // `.DELETE_ON_ERROR:` is a global flag (its prerequisites are ignored).
                ".DELETE_ON_ERROR" => self.special.delete_on_error = true,
                // `.SUFFIXES:` with no prerequisites clears the suffix list; otherwise it appends.
                ".SUFFIXES" => {
                    if rule.prerequisites.is_empty() {
                        self.suffixes.clear();
                    } else {
                        self.suffixes.extend(rule.prerequisites.iter().cloned());
                    }
                }
                _ => {}
            }
        }
    }

    /// Whether `target` is `.PHONY` (always considered out of date).
    fn is_phony(&self, target: &str) -> bool {
        self.special.phony.contains(target)
    }

    /// Whether `target`'s recipes should not be echoed (via `.SILENT`).
    fn is_silent(&self, target: &str) -> bool {
        self.special.silent_all || self.special.silent.contains(target)
    }

    /// Whether recipe errors for `target` should be ignored (via `.IGNORE`).
    fn is_ignored(&self, target: &str) -> bool {
        self.special.ignore_all || self.special.ignore.contains(target)
    }

    /// The recipe defined by `.DEFAULT`, used for targets that have no rule of their own.
    fn default_recipe(&self) -> &[String] {
        &self.special.default_recipe
    }

    /// Whether `target` is `.PRECIOUS` (exempt from delete-on-error). An exact name always matches.
    /// A `%` pattern entry (e.g. `%.o`) matches only when the target is built by a pattern rule
    /// (`pattern_build`), which is how gmake scopes pattern entries in `.PRECIOUS`; a bare `%`
    /// protects nothing.
    fn is_precious(&self, target: &str, pattern_build: bool) -> bool {
        if self.special.precious.contains(target) {
            return true;
        }
        pattern_build
            && self
                .special
                .precious
                .iter()
                .any(|p| precious_pattern_matches(p, target))
    }

    /// Whether `.DELETE_ON_ERROR:` is in effect.
    fn delete_on_error(&self) -> bool {
        self.special.delete_on_error
    }

    /// Get the `mtime` of a file. Note that the return value also signals whether or not the file
    /// is accessible, so a `None` value represents either the file not existing or the current user
    /// not having the appropriate permissions to access the file.
    ///
    /// TODO: Consider bailing on a file permissions issue? Not sure if POSIX specifies some
    /// behavior here or if the major implementations halt execution on a permissions error.
    fn get_mtime(&self, file: &str) -> Option<SystemTime> {
        // Phony targets are never up to date, so their recipes always run.
        if self.is_phony(file) {
            return None;
        }

        // Resolve through `VPATH`/`vpath` so a file found in a search directory counts as existing.
        let path = self.find_file(file)?;

        match fs::metadata(&path) {
            Ok(metadata) => {
                if self.opts.old_file.iter().any(|f| f == file) {
                    Some(UNIX_EPOCH)
                } else if self.opts.new_file.iter().any(|f| f == file) {
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
    /// Whether the logical line is a recipe (the first physical line starts with the recipe
    /// prefix).
    is_recipe: bool,
    /// The recipe prefix in effect when this logical line began.
    recipe_prefix: String,
    /// The accumulated logical line so far.
    accum: String,
}

/// If `target` names a suffix rule (`.s2` single-suffix or `.s1.s2` double-suffix, where each `.sN`
/// is a known suffix), return the equivalent pattern rule as `(target_pattern, prereq_pattern)`.
fn suffix_rule_patterns(target: &str, suffixes: &[String]) -> Option<(String, String)> {
    // Single-suffix rule `.c`: build `%` from `%.c`.
    if suffixes.iter().any(|s| s == target) {
        return Some(("%".to_string(), format!("%{target}")));
    }
    // Double-suffix rule `.c.o`: source suffix `.c`, target suffix `.o` -> `%.o: %.c`.
    for source in suffixes {
        if let Some(rest) = target.strip_prefix(source.as_str())
            && suffixes.iter().any(|t| t == rest)
        {
            return Some((format!("%{rest}"), format!("%{source}")));
        }
    }
    None
}

/// If `trimmed` is a `vpath` directive, return the part after `vpath`.
fn parse_vpath(trimmed: &str) -> Option<&str> {
    if let Some(rest) = trimmed.strip_prefix("vpath")
        && (rest.is_empty() || rest.starts_with([' ', '\t']))
    {
        return Some(rest.trim());
    }
    None
}

/// Split a directory list separated by colons or whitespace (as in `VPATH`/`vpath`).
fn split_path_list(list: &str) -> Vec<String> {
    list.split([':', ' ', '\t'])
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Whether a `vpath` pattern matches `name`. A pattern with `%` matches on prefix/suffix; without
/// `%` it must equal `name`.
fn vpath_matches(pattern: &str, name: &str) -> bool {
    match pattern.find('%') {
        Some(pct) => {
            let (prefix, suffix) = (&pattern[..pct], &pattern[pct + 1..]);
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
        None => pattern == name,
    }
}

/// The stem that `%` matches when `target` fits a pattern split into `prefix`/`suffix` (the parts
/// around its `%`). Returns `None` if the target does not match.
fn pattern_stem<'a>(prefix: &str, suffix: &str, target: &'a str) -> Option<&'a str> {
    if target.len() < prefix.len() + suffix.len() {
        return None;
    }
    target.strip_prefix(prefix)?.strip_suffix(suffix)
}

/// Whether a `.PRECIOUS` pattern entry (which must contain `%`) matches `target`. A bare `%` matches
/// nothing, since gmake does not let the catch-all pattern protect any target from delete-on-error.
fn precious_pattern_matches(pattern: &str, target: &str) -> bool {
    let Some(pct) = pattern.find('%') else {
        return false;
    };
    let (prefix, suffix) = (&pattern[..pct], &pattern[pct + 1..]);
    if prefix.is_empty() && suffix.is_empty() {
        return false;
    }
    target.len() >= prefix.len() + suffix.len()
        && target.starts_with(prefix)
        && target.ends_with(suffix)
}

/// If `trimmed` is a `define NAME [op]` directive, return the part after `define`.
fn parse_define(trimmed: &str) -> Option<&str> {
    if let Some(rest) = trimmed.strip_prefix("define")
        && (rest.is_empty() || rest.starts_with([' ', '\t']))
    {
        return Some(rest.trim());
    }
    None
}

/// If `trimmed` is an `export`/`unexport` directive, return whether it exports (vs unexports) and
/// the remainder of the line.
fn parse_export(trimmed: &str) -> Option<(bool, &str)> {
    for (kw, export) in [("export", true), ("unexport", false)] {
        if let Some(rest) = trimmed.strip_prefix(kw)
            && (rest.is_empty() || rest.starts_with([' ', '\t']))
        {
            return Some((export, rest));
        }
    }
    None
}

/// If `trimmed` is an `undefine NAME` directive (optionally `override undefine NAME`), return
/// whether it was `override` and the (unexpanded) name argument — the whole rest of the line, not
/// split. `undefine` must be followed by whitespace and a non-empty argument.
fn parse_undefine(trimmed: &str) -> Option<(bool, &str)> {
    let (override_, rest) = match trimmed.strip_prefix("override") {
        Some(r) if r.starts_with([' ', '\t']) => (true, r.trim_start()),
        _ => (false, trimmed),
    };
    let name = rest.strip_prefix("undefine")?;
    name.starts_with([' ', '\t'])
        .then(|| (override_, name.trim()))
}

/// If `trimmed` is an `include`/`-include`/`sinclude` directive, return whether missing files are
/// tolerated and the (unexpanded) filename list.
fn parse_include(trimmed: &str) -> Option<(bool, &str)> {
    for (kw, optional) in [("include", false), ("-include", true), ("sinclude", true)] {
        if let Some(rest) = trimmed.strip_prefix(kw)
            && (rest.is_empty() || rest.starts_with([' ', '\t']))
        {
            return Some((optional, rest.trim()));
        }
    }
    None
}

/// If `trimmed` is a BSD `.include "file"` / `.include <file>` directive (or the `.-include` /
/// `.sinclude` optional variants), return whether missing files are tolerated and the (unexpanded)
/// filename. The system-header `<...>` form is treated like the quoted form.
fn parse_bsd_include(trimmed: &str) -> Option<(bool, &str)> {
    for (kw, optional) in [
        (".include", false),
        (".-include", true),
        (".sinclude", true),
    ] {
        if let Some(rest) = trimmed.strip_prefix(kw)
            && (rest.is_empty() || rest.starts_with([' ', '\t']))
        {
            let rest = rest.trim();
            let name = rest
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| rest.strip_prefix('<').and_then(|s| s.strip_suffix('>')))?;
            return Some((optional, name));
        }
    }
    None
}

/// Split a BSD `.if` expression on top-level occurrences of the logical operator `op` (`&&`/`||`),
/// ignoring operators inside parentheses. To match bmake, the operator is only recognized when
/// preceded by whitespace or a closing paren — so a run like `1&&0` stays a single bare term.
/// Returns `None` if the operator does not appear at top level.
fn split_logical<'a>(expr: &'a str, op: &str) -> Option<Vec<&'a str>> {
    let bytes = expr.as_bytes();
    let mut depth: i32 = 0;
    let mut in_quote = false;
    let mut segments = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_quote = !in_quote,
            _ if in_quote => {}
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ if depth == 0
                && i > 0
                && matches!(bytes[i - 1], b' ' | b'\t' | b')')
                && expr[i..].starts_with(op) =>
            {
                segments.push(&expr[start..i]);
                i += op.len();
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    if segments.is_empty() {
        return None;
    }
    segments.push(&expr[start..]);
    Some(segments)
}

/// If `expr` is a single parenthesized group wrapping the whole string (`(...)`), return the inner
/// text. Returns `None` when the leading `(` closes before the end (e.g. `(a) && (b)`).
fn strip_full_parens(expr: &str) -> Option<&str> {
    if !expr.starts_with('(') {
        return None;
    }
    let bytes = expr.as_bytes();
    let mut depth: i32 = 0;
    let mut in_quote = false;
    for (i, &c) in bytes.iter().enumerate() {
        match c {
            b'"' => in_quote = !in_quote,
            _ if in_quote => {}
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return (i == bytes.len() - 1).then(|| &expr[1..i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `name` is a valid environment-variable identifier (`[A-Za-z_][A-Za-z0-9_]*`). Export-all
/// only exports variables with such names (so dotted/hyphenated names are skipped).
fn is_env_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Expand any glob patterns among `words` against the filesystem. A word with a metacharacter is
/// replaced by its (sorted) matches; if it matches nothing, the literal word is kept (matching GNU
/// make). Words with no metacharacters pass through untouched.
fn glob_words(words: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    for word in words {
        if crate::glob::has_glob_chars(&word) {
            let matches = crate::glob::glob(&word);
            if matches.is_empty() {
                out.push(word);
            } else {
                out.extend(matches);
            }
        } else {
            out.push(word);
        }
    }
    out
}

/// Parse a BSD numeric literal: decimal or float (`5`, `05`, `1.5`), or hex (`0x10`). Returns
/// `None` for a non-number, which signals a string comparison.
fn bsd_number(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).ok().map(|n| n as f64);
    }
    s.parse::<f64>().ok()
}

/// If `trimmed` is a BSD conditional directive, return its canonical keyword and the (unexpanded)
/// argument. BSD allows whitespace after the leading dot to indent nested conditionals
/// (e.g. `.  ifdef`), so the dot and keyword are matched separately.
fn bsd_cond_keyword(trimmed: &str) -> Option<(&'static str, &str)> {
    let body = trimmed.strip_prefix('.')?.trim_start();
    // Longest-first so `ifdef` is not shadowed by `if`, `elifdef` not by `elif`, etc.
    for (canonical, kw) in [
        (".ifndef", "ifndef"),
        (".ifdef", "ifdef"),
        (".elifndef", "elifndef"),
        (".elifdef", "elifdef"),
        (".elif", "elif"),
        (".endif", "endif"),
        (".else", "else"),
        (".if", "if"),
    ] {
        if body == kw
            || (body.starts_with(kw) && body[kw.len()..].starts_with([' ', '\t', '(', '!']))
        {
            return Some((canonical, body[kw.len()..].trim()));
        }
    }
    None
}

/// If `trimmed` begins with a conditional directive keyword (as a whole word), return it.
fn directive_keyword(trimmed: &str) -> Option<&'static str> {
    for kw in ["ifeq", "ifneq", "ifdef", "ifndef", "else", "endif"] {
        if trimmed == kw
            || (trimmed.starts_with(kw) && trimmed[kw.len()..].starts_with([' ', '\t', '(']))
        {
            return Some(kw);
        }
    }
    None
}

/// Parse the two arguments of an `ifeq`/`ifneq`, in either `(a,b)` or `'a' 'b'` / `"a" "b"` form.
fn parse_ifeq_args(rest: &str) -> Option<(String, String)> {
    let rest = rest.trim();

    if let Some(inner) = rest.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        let comma = top_level_comma(inner)?;
        return Some((
            inner[..comma].trim().to_string(),
            inner[comma + 1..].trim().to_string(),
        ));
    }

    let (a, remainder) = parse_quoted(rest)?;
    let (b, _) = parse_quoted(remainder.trim_start())?;
    Some((a, b))
}

/// Index of the first top-level comma (ignoring commas nested inside `(...)`/`{...}`).
fn top_level_comma(s: &str) -> Option<usize> {
    let mut depth: usize = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '{' => depth += 1,
            ')' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Parse a single-quoted or double-quoted string from the start of `s`, returning it (without the
/// quotes) and the remainder of the input.
fn parse_quoted(s: &str) -> Option<(String, &str)> {
    let quote = s.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let after = &s[quote.len_utf8()..];
    let end = after.find(quote)?;
    Some((after[..end].to_string(), &after[end + quote.len_utf8()..]))
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
    let mut chars = line.chars().peekable();
    // Stack of the opener chars (`(`/`{`) of the enclosing `$(...)` / `${...}` references. A `#`
    // inside a reference is literal, not a comment (GNU make behavior), so `$(shell echo a#b)` and
    // URLs like `http://x#frag` survive. Only the opener's own delimiter closes it, so a stray `}`
    // inside `$(...)` (or `)` inside `${...}`) is literal.
    let mut refs: Vec<char> = Vec::new();

    while let Some(c) = chars.next() {
        match c {
            '#' if refs.is_empty() => break,
            '\\' if refs.is_empty() => match chars.next() {
                Some('#') => out.push('#'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            '$' => {
                out.push(c);
                if matches!(chars.peek(), Some('(') | Some('{')) {
                    let open = chars.next().unwrap();
                    out.push(open);
                    refs.push(open);
                }
            }
            ')' if refs.last() == Some(&'(') => {
                refs.pop();
                out.push(c);
            }
            '}' if refs.last() == Some(&'{') => {
                refs.pop();
                out.push(c);
            }
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

/// Detect a top-level variable assignment in `line`, returning `(key, op, value)`, or `None` if
/// the line is not an assignment (e.g. it is a rule). The operator is found before any rule colon:
/// a `:` that is not part of `:=`/`::=` means the line is a rule. Text inside `$(...)`/`${...}` is
/// skipped so operators there are not mistaken for the assignment. `key`/`value` are not trimmed
/// here.
fn parse_assignment(line: &str) -> Option<(&str, AssignOp, &str)> {
    let bytes = line.as_bytes();
    let mut depth: usize = 0;

    for i in 0..bytes.len() {
        let c = bytes[i];

        // Inside a `$(...)`/`${...}` reference we only balance delimiters; operators there belong
        // to the reference, not the assignment.
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
