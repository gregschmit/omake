//! Data structures for makefile rules.

use std::collections::{HashMap, HashSet};
use std::process::Command;

use super::{Context, Logger, MakeError, Makefile};
use crate::expand::expand;
use crate::vars::Vars;

/// Maximum length of a chained implicit-rule search (e.g. `%.o: %.c` then `%.c: %.y`).
const PATTERN_CHAIN_LIMIT: usize = 16;

/// A parsed rule from a makefile.
#[derive(Debug, Clone)]
pub struct Rule {
    pub targets: Vec<String>,
    pub prerequisites: Vec<String>,
    /// Order-only prerequisites (after `|`): made if missing, but their mtime never forces a rebuild.
    pub order_only: Vec<String>,
    pub recipe: Vec<String>,
    pub context: Context,
    pub double_colon: bool,
}

impl Rule {
    /// Execute this rule's recipe for a specific `target` (one rule may have several targets, and
    /// automatic variables like `$@` depend on which one is being built). `stem` is the `%` match
    /// for a pattern rule (used for `$*`); it is empty for explicit rules.
    pub fn execute<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        stem: &str,
    ) -> Result<(), MakeError> {
        // Build a per-target variable scope: the makefile's variables plus automatic variables.
        let mut vars = makefile.vars.clone();
        self.set_automatic_vars(&mut vars, makefile, target, stem);

        let shell = vars.get("SHELL").value.clone();
        let shell_flags = vars
            .get(".SHELLFLAGS")
            .value
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        // `.SILENT`/`.IGNORE` apply to every recipe line of the target.
        let silent_target = makefile.is_silent(target);
        let ignore_target = makefile.is_ignored(target);

        for raw in self.recipe.iter() {
            // Expand the recipe line now, at execution time. This resolves variables defined after
            // the rule, automatic variables, and cases where a variable expands to a command
            // modifier (e.g. the kernel's `$(Q)` = `@` pattern), which must be recognized below.
            let line = expand(raw, &vars).map_err(|e| MakeError::new(&e, self.context.clone()))?;

            // Strip leading command modifiers: `@` (silence echo), `-` (ignore errors), and `+`
            // (run even under `--just-print`). They may be combined in any order and separated by
            // whitespace (e.g. `@-` or `- @`), and must be removed before the command is echoed or
            // handed to the shell; only the remainder is a real command.
            let mut silent = false;
            let mut ignore_errors = false;
            let mut force = false;
            let mut command = line.as_str();
            loop {
                command = command.trim_start();
                match command.chars().next() {
                    Some('@') => silent = true,
                    Some('-') => ignore_errors = true,
                    Some('+') => force = true,
                    _ => break,
                }
                command = &command[1..];
            }

            // A recipe consisting only of modifiers (e.g. `@`) is a no-op.
            if command.is_empty() {
                continue;
            }

            // Echo the command to stdout, unless silenced. `--just-print` always echoes.
            if (!silent && !silent_target) || makefile.opts.just_print {
                println!("{command}");
            }

            // Under `--just-print` we only echo and move on, except `+` commands always run.
            if makefile.opts.just_print && !force {
                continue;
            }

            // Execute the recipe line.
            let res = Command::new(&shell)
                .args(&shell_flags)
                .arg(command)
                .status()
                .map_err(|e| MakeError::new(&e.to_string(), self.context.clone()))?;

            // Check for command errors, unless directed to ignore them.
            if !ignore_errors && !ignore_target && !makefile.opts.ignore_errors {
                match res.code() {
                    Some(0) => {}
                    Some(code) => {
                        return Err(MakeError::new(
                            &format!("Failed with code {code}."),
                            self.context.clone(),
                        ));
                    }
                    None => return Err(MakeError::new("Killed.", self.context.clone())),
                }
            }
        }

        Ok(())
    }

    /// Populate `vars` with the automatic variables for building `target` (with `stem` as `$*`).
    fn set_automatic_vars<L: Logger>(
        &self,
        vars: &mut Vars,
        makefile: &Makefile<L>,
        target: &str,
        stem: &str,
    ) {
        // `$@`: the target being built.
        let _ = vars.set("@", target, false);

        // `$<`: the first prerequisite.
        let first = self.prerequisites.first().map(String::as_str).unwrap_or("");
        let _ = vars.set("<", first, false);

        // `$^`: all prerequisites, de-duplicated with order preserved.
        let mut seen = HashSet::new();
        let deduped = self
            .prerequisites
            .iter()
            .filter(|p| seen.insert(p.as_str()))
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        let _ = vars.set("^", deduped.as_str(), false);

        // `$+`: all prerequisites, keeping duplicates.
        let all = self.prerequisites.join(" ");
        let _ = vars.set("+", all.as_str(), false);

        // `$?`: prerequisites newer than the target, de-duplicated. If the target is absent, every
        // prerequisite counts; if the target exists, a prerequisite counts when its file is newer or
        // when it has no file (phony / just-rebuilt prerequisites are treated as newer).
        let target_mtime = makefile.get_mtime(target);
        let mut seen_newer = HashSet::new();
        let newer = self
            .prerequisites
            .iter()
            .filter(|p| match target_mtime {
                None => true,
                Some(tm) => makefile.get_mtime(p).is_none_or(|pm| pm > tm),
            })
            .filter(|p| seen_newer.insert(p.as_str()))
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        let _ = vars.set("?", newer.as_str(), false);

        // `$|`: the order-only prerequisites.
        let order_only = self.order_only.join(" ");
        let _ = vars.set("|", order_only.as_str(), false);

        // `$*`: the stem matched by a pattern rule's `%` (empty for explicit rules).
        let _ = vars.set("*", stem, false);

        // Directory/file parts of `$@` and `$<`.
        let (at_dir, at_file) = split_dir_file(target);
        let _ = vars.set("@D", at_dir.as_str(), false);
        let _ = vars.set("@F", at_file.as_str(), false);
        let (lt_dir, lt_file) = split_dir_file(first);
        let _ = vars.set("<D", lt_dir.as_str(), false);
        let _ = vars.set("<F", lt_file.as_str(), false);
    }
}

/// Split a path into its directory part (`.` when there is no slash, trailing slash removed) and its
/// file part, matching make's `$(@D)`/`$(@F)`.
fn split_dir_file(path: &str) -> (String, String) {
    match path.rfind('/') {
        Some(i) => (path[..i].to_string(), path[i + 1..].to_string()),
        None => (".".to_string(), path.to_string()),
    }
}

/// A pattern rule (e.g. `%.o: %.c`). The target pattern is split around its single `%` into a
/// prefix and suffix; the stem is whatever `%` matched.
#[derive(Debug, Clone)]
struct PatternRule {
    target_prefix: String,
    target_suffix: String,
    /// Prerequisite patterns; a `%` in one is replaced by the matched stem.
    prereq_patterns: Vec<String>,
    /// Order-only prerequisite patterns.
    order_only_patterns: Vec<String>,
    recipe: Vec<String>,
    context: Context,
}

impl PatternRule {
    /// If `target` matches this rule's target pattern, return the stem that `%` matched.
    fn match_stem<'a>(&self, target: &'a str) -> Option<&'a str> {
        if target.len() < self.target_prefix.len() + self.target_suffix.len() {
            return None;
        }
        target
            .strip_prefix(&self.target_prefix)?
            .strip_suffix(&self.target_suffix)
    }

    /// The concrete prerequisites for a given `stem`.
    fn prerequisites(&self, stem: &str) -> Vec<String> {
        self.prereq_patterns
            .iter()
            .map(|p| p.replace('%', stem))
            .collect()
    }

    /// The concrete order-only prerequisites for a given `stem`.
    fn order_only(&self, stem: &str) -> Vec<String> {
        self.order_only_patterns
            .iter()
            .map(|p| p.replace('%', stem))
            .collect()
    }
}

/// Wrapper for a mapping of targets to rules, and with a facility to execute targets.
#[derive(Debug)]
pub struct RuleMap {
    /// Storage for added rules. Rules MUST only be inserted, as removal may invalidate items in
    /// `by_target`.
    rules: Vec<Rule>,

    /// Map targets (strings) to the rules which reference them by index into `self.rules`.
    by_target: HashMap<String, Vec<usize>>,

    /// Pattern rules (targets containing `%`), tried in order when no explicit rule matches.
    pattern_rules: Vec<PatternRule>,
}

/// Note that methods on `RuleMap` MUST ensure that only new entries are added to either `rules` or
/// `by_target` to ensure index references always remain valid. Also, entries added to `by_target`
/// must always initialize with at least one index, never an empty vector.
impl Default for RuleMap {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleMap {
    pub fn new() -> Self {
        Self {
            rules: vec![],
            by_target: HashMap::new(),
            pattern_rules: vec![],
        }
    }

    /// Insert a rule, update the `by_target` hashmap, and validate the rule.
    pub fn insert<L: Logger>(&mut self, rule: Rule, logger: &L) -> Result<(), MakeError> {
        // A rule with a `%` target is a pattern rule, stored separately from exact-match rules.
        if rule.targets.iter().any(|t| t.contains('%')) {
            for target in &rule.targets {
                if let Some(pct) = target.find('%') {
                    self.pattern_rules.push(PatternRule {
                        target_prefix: target[..pct].to_string(),
                        target_suffix: target[pct + 1..].to_string(),
                        prereq_patterns: rule.prerequisites.clone(),
                        order_only_patterns: rule.order_only.clone(),
                        recipe: rule.recipe.clone(),
                        context: rule.context.clone(),
                    });
                }
            }
            return Ok(());
        }

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
                        logger.warn("Ignoring duplicate definition.", Some(&rule.context));
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute the given `targets` in order, sharing build state so each is built at most once and
    /// circular dependencies are detected rather than recursed into forever.
    pub fn execute<L>(&self, makefile: &Makefile<L>, targets: &[String]) -> Result<(), MakeError>
    where
        L: Logger,
    {
        let mut state = ExecState::default();
        for target in targets {
            // Top-level targets have no parent edge (they cannot be circular on first entry).
            self.execute_target(makefile, target, "", &mut state)?;
        }
        Ok(())
    }

    /// Build a single target, guarding against re-building (memoization) and circular dependencies.
    /// `parent` is the target that required this one (for the circular-dependency diagnostic).
    fn execute_target<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        parent: &str,
        state: &mut ExecState,
    ) -> Result<(), MakeError> {
        // Already resolved this run: nothing to do.
        if state.done.contains(target) {
            return Ok(());
        }

        // Already on the current stack: a circular dependency. GNU make warns and drops the edge
        // that closes the cycle (`parent <- target`) rather than looping forever, so do the same.
        if !state.in_progress.insert(target.to_string()) {
            makefile.logger.warn(
                &format!("Circular {parent} <- {target} dependency dropped."),
                None,
            );
            return Ok(());
        }

        let result = self.build_target(makefile, target, state);

        state.in_progress.remove(target);
        if result.is_ok() {
            state.done.insert(target.to_string());
        }
        result
    }

    /// The core build logic for one target: resolve prerequisites, then run recipes if out of date.
    fn build_target<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        state: &mut ExecState,
    ) -> Result<(), MakeError> {
        let rule_indices = match self.by_target.get(target) {
            Some(indices) => indices,
            None => {
                // No explicit rule. Try pattern rules first (they can rebuild an existing target
                // when a prerequisite is newer).
                if let Some((idx, stem)) = self.match_pattern_rule(makefile, target) {
                    return self.build_from_pattern(makefile, target, idx, &stem, state);
                }
                // An existing file is a source and is up to date; a target declared only via
                // `.PHONY` is a valid empty target; otherwise fall back to `.DEFAULT`, or error.
                if makefile.get_mtime(target).is_some() || makefile.is_phony(target) {
                    return Ok(());
                }
                let default = makefile.default_recipe();
                if !default.is_empty() {
                    let rule = Rule {
                        targets: vec![target.to_string()],
                        prerequisites: vec![],
                        order_only: vec![],
                        recipe: default.to_vec(),
                        context: Context::new(),
                        double_colon: false,
                    };
                    return rule.execute(makefile, target, "");
                }
                return Err(MakeError::new(
                    &format!("No rule to make target '{target}'."),
                    Context::new(),
                ));
            }
        };

        // Old files have their rules ignored.
        if makefile.opts.old_file.iter().any(|f| f == target) {
            makefile
                .logger
                .info(&format!("Target '{target}' is up to date (old)."), None);
            return Ok(());
        }

        let mut executed = false;
        for i in rule_indices {
            let rule = &self.rules[i.to_owned()];
            // Re-read the target mtime per rule so a double-colon rule sees files written by an
            // earlier rule for the same target.
            let target_mtime_opt = makefile.get_mtime(target);
            let mut should_execute = makefile.opts.always_make;

            // Check (and possibly execute) prereqs.
            for prereq in &rule.prerequisites {
                // Check if prereq exists unless `always_make`.
                if makefile.opts.always_make {
                    self.execute_target(makefile, prereq, target, state)?;
                } else {
                    match makefile.get_mtime(prereq) {
                        None => {
                            // Prereq doesn't exist, so make it. By definition, it's more up-to-date
                            // than the target.
                            self.execute_target(makefile, prereq, target, state)?;
                            should_execute = true;
                        }
                        Some(prereq_mtime) => {
                            // Prereq exists, so check if it's more up-to-date than the target.
                            if let Some(target_mtime) = target_mtime_opt
                                && prereq_mtime > target_mtime
                            {
                                should_execute = true;
                            }
                        }
                    }
                }
            }

            // Order-only prerequisites are made if missing but never force a rebuild.
            self.make_order_only(makefile, &rule.order_only, target, state)?;

            if target_mtime_opt.is_none() || should_execute {
                rule.execute(makefile, target, "")?;
                executed = true;
            }
        }

        if !executed {
            makefile.logger.info(
                &format!("Target '{target}' is up to date."),
                Some(&Context::new()),
            );
        }

        Ok(())
    }

    /// Find the first pattern rule whose target pattern matches `target` and whose (stem-expanded)
    /// prerequisites can all be made. Returns the rule's index and the matched stem.
    fn match_pattern_rule<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
    ) -> Option<(usize, String)> {
        for (idx, prule) in self.pattern_rules.iter().enumerate() {
            if let Some(stem) = prule.match_stem(target) {
                let prereqs = prule.prerequisites(stem);
                let order_only = prule.order_only(stem);
                if prereqs
                    .iter()
                    .chain(&order_only)
                    .all(|p| self.can_be_made(makefile, p, PATTERN_CHAIN_LIMIT))
                {
                    return Some((idx, stem.to_string()));
                }
            }
        }
        None
    }

    /// Whether `name` can serve as a prerequisite when choosing a pattern rule: it already exists,
    /// has an explicit rule, is phony, or can be produced by another pattern rule (chained implicit
    /// rules, e.g. `%.o: %.c` with `%.c: %.y`). `depth` bounds the chain to avoid infinite recursion.
    fn can_be_made<L: Logger>(&self, makefile: &Makefile<L>, name: &str, depth: usize) -> bool {
        if makefile.get_mtime(name).is_some()
            || self.by_target.contains_key(name)
            || makefile.is_phony(name)
        {
            return true;
        }
        if depth == 0 {
            return false;
        }
        self.pattern_rules
            .iter()
            .any(|prule| match prule.match_stem(name) {
                Some(stem) => prule
                    .prerequisites(stem)
                    .iter()
                    .all(|p| self.can_be_made(makefile, p, depth - 1)),
                None => false,
            })
    }

    /// Build `target` using the pattern rule at `idx` with the given `stem`.
    fn build_from_pattern<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        idx: usize,
        stem: &str,
        state: &mut ExecState,
    ) -> Result<(), MakeError> {
        let prule = &self.pattern_rules[idx];
        let prereqs = prule.prerequisites(stem);
        let order_only = prule.order_only(stem);
        let target_mtime = makefile.get_mtime(target);
        let mut should_execute = makefile.opts.always_make;

        for prereq in &prereqs {
            if makefile.opts.always_make {
                self.execute_target(makefile, prereq, target, state)?;
            } else {
                match makefile.get_mtime(prereq) {
                    None => {
                        self.execute_target(makefile, prereq, target, state)?;
                        should_execute = true;
                    }
                    Some(prereq_mtime) => {
                        if let Some(tm) = target_mtime
                            && prereq_mtime > tm
                        {
                            should_execute = true;
                        }
                    }
                }
            }
        }

        self.make_order_only(makefile, &order_only, target, state)?;

        if target_mtime.is_none() || should_execute {
            let rule = Rule {
                targets: vec![target.to_string()],
                prerequisites: prereqs,
                order_only,
                recipe: prule.recipe.clone(),
                context: prule.context.clone(),
                double_colon: false,
            };
            rule.execute(makefile, target, stem)?;
        }

        Ok(())
    }

    /// Bring order-only prerequisites up to date. They are resolved like any other prerequisite;
    /// "order-only" only means their timestamp does not force the parent target to rebuild (handled
    /// by the caller not consulting their mtime).
    fn make_order_only<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        order_only: &[String],
        parent: &str,
        state: &mut ExecState,
    ) -> Result<(), MakeError> {
        for prereq in order_only {
            self.execute_target(makefile, prereq, parent, state)?;
        }
        Ok(())
    }
}

/// Mutable state threaded through a single top-level execution.
#[derive(Default)]
struct ExecState {
    /// Targets currently on the recursion stack, used to detect circular dependencies.
    in_progress: HashSet<String>,
    /// Targets already resolved this run, so a shared prerequisite is built at most once.
    done: HashSet<String>,
}
