//! Data structures for makefile rules.

use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::time::SystemTime;

use super::{Context, Logger, MakeError, Makefile};
use crate::expand::expand_collect;
use crate::vars::Vars;

/// Maximum length of a chained implicit-rule search (e.g. `%.o: %.c` then `%.c: %.y`).
const PATTERN_CHAIN_LIMIT: usize = 16;

/// Built-in implicit pattern rules (GNU make defaults), registered after the makefile is parsed so
/// that user rules take precedence. Recipes reference the standard `COMPILE.*`/`LINK.*` variables.
const BUILTIN_RULES: &[(&str, &[&str], &str)] = &[
    ("%.o", &["%.c"], "$(COMPILE.c) $(OUTPUT_OPTION) $<"),
    ("%.o", &["%.cc"], "$(COMPILE.cc) $(OUTPUT_OPTION) $<"),
    ("%.o", &["%.cpp"], "$(COMPILE.cpp) $(OUTPUT_OPTION) $<"),
    ("%.o", &["%.C"], "$(COMPILE.C) $(OUTPUT_OPTION) $<"),
    ("%", &["%.c"], "$(LINK.c) $^ $(LOADLIBES) $(LDLIBS) -o $@"),
    ("%", &["%.cc"], "$(LINK.cc) $^ $(LOADLIBES) $(LDLIBS) -o $@"),
    ("%", &["%.o"], "$(LINK.o) $^ $(LOADLIBES) $(LDLIBS) -o $@"),
];

/// Prerequisites borrowed from a recipe-less explicit rule when its target uses a matching pattern
/// rule's recipe (GNU make's implicit-rule-with-explicit-prerequisites behaviour).
#[derive(Default)]
struct BorrowedPrereqs {
    normal: Vec<String>,
    order_only: Vec<String>,
}

/// A parsed rule from a makefile.
#[derive(Debug, Clone)]
pub struct Rule {
    pub targets: Vec<String>,
    pub prerequisites: Vec<String>,
    /// Order-only prerequisites (after `|`): made if missing, but their mtime never forces a
    /// rebuild.
    pub order_only: Vec<String>,
    pub recipe: Vec<String>,
    /// 0-based source line index of each stored recipe entry (parallel to `recipe`), used to report
    /// the failing recipe line on error. Kept in lockstep with `recipe`.
    pub recipe_lines: Vec<usize>,
    /// Whether a recipe section was present, even if it holds no command lines (e.g. `target:;` or
    /// a target followed by a blank recipe line). GNU make treats such a target as "having commands"
    /// — it reports it up to date rather than "Nothing to be done" — so this is tracked separately
    /// from `recipe` being non-empty.
    pub has_recipe: bool,
    /// The `%` stem for `$*`, set for a static pattern rule (`targets: %.o: %.c`); empty otherwise.
    pub stem: String,
    /// The target pattern of an unexpanded static pattern rule. Present only transiently during
    /// parsing; each listed target is expanded into its own concrete rule (with `stem` filled in)
    /// before insertion, so every stored rule has `None` here.
    pub target_pattern: Option<String>,
    /// Whether this rule's prerequisites need a second expansion at build time (`.SECONDEXPANSION:`).
    /// When set, `prerequisites`/`order_only` hold the once-expanded, un-split prerequisite text.
    pub second_expand: bool,
    pub context: Context,
    pub double_colon: bool,
}

/// The result of running (or, under `-q`, question-checking) a rule's recipe.
#[derive(Default)]
pub struct RecipeOutcome {
    /// Command lines echoed or run — feeds the "up to date" / "Nothing to be done" summary.
    pub commands_run: usize,
    /// Under `-q`, whether the target was found out of date (a non-`+` recipe line was reached, or a
    /// forced `+` line failed). Always false in normal mode.
    pub needs_rebuild: bool,
}

impl Rule {
    /// Execute this rule's recipe for a specific `target` (one rule may have several targets, and
    /// automatic variables like `$@` depend on which one is being built). `self.stem` is the `%`
    /// match for `$*`. `pattern_build` is true only for a target built by an implicit pattern rule
    /// (it scopes `.PRECIOUS` pattern entries, which do not protect static-pattern/explicit targets).
    /// Returns the number of command lines actually echoed or run (lines that were only command
    /// modifiers such as `@` execute nothing and are not counted). The caller uses this to decide
    /// whether any real work happened for the "up to date" / "Nothing to be done" summary.
    pub fn execute<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        pattern_build: bool,
        scope: &Vars,
    ) -> Result<RecipeOutcome, MakeError> {
        // Build a per-target variable scope: the incoming scope (makefile variables plus any
        // target-specific ones) plus automatic variables. `private` target-specific variables are
        // added here — they scope to this recipe only, so they are excluded from the inherited scope
        // that reached the prerequisites.
        let mut vars = scope.clone();
        if makefile.has_private_target_vars(target) {
            makefile.apply_private_target_vars(&mut vars, target);
        }
        self.set_automatic_vars(&mut vars, makefile, target, &self.stem);

        let mut commands_run = 0;
        let needs_rebuild = false;

        // For `.DELETE_ON_ERROR:`, remember the target file's state so a failing recipe can tell
        // whether it modified the target (and should therefore delete it).
        let mtime_before = std::fs::metadata(target)
            .ok()
            .and_then(|m| m.modified().ok());

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

        // `export`ed variables are added to each recipe's child environment; `unexport`ed names are
        // removed (even if inherited from omake's own environment).
        let (env_adds, env_removes) = makefile.exported_env(&vars);

        // Under `-t` (touch mode) the recipe is replaced by a single `touch <target>`: only
        // `+`-prefixed lines still run, the rest are skipped, and the target is touched afterward.
        let touch_mode = makefile.opts.touch;
        let mut touch_saw_plus = false;
        let mut touch_saw_non_plus = false;

        debug_assert_eq!(self.recipe.len(), self.recipe_lines.len());
        for (raw, &recipe_line) in self.recipe.iter().zip(self.recipe_lines.iter()) {
            // Command modifiers (`@` silence, `-` ignore errors, `+` run even under `--just-print`)
            // that appear literally at the start of the recipe line — before expansion — apply to
            // EVERY line the recipe expands to, so `@$(canned)` silences all of a multi-line canned
            // recipe. Modifiers introduced by expansion (e.g. the kernel's `$(Q)` = `@`) apply only
            // to their own line. This matches GNU make.
            let (line_silent, line_ignore, line_force, body) = strip_command_modifiers(raw);

            if touch_mode {
                if line_force {
                    touch_saw_plus = true; // a `+` line runs even under -t
                } else {
                    touch_saw_non_plus = true;
                    continue;
                }
            }

            // Expand the recipe body now, at execution time. This resolves variables defined after
            // the rule, automatic variables, and cases where a variable expands to a command
            // modifier, which is recognized per line below.
            let expansion = expand_collect(body, &vars)
                .map_err(|e| MakeError::new(&e, self.context.clone()))?;
            // A `$(shell ...)` run while expanding this line records its exit code in `.SHELLSTATUS`
            // for a later recipe line to read.
            if let Some(code) = expansion.shell_status {
                let _ = vars.set(".SHELLSTATUS", code.to_string().as_str(), false);
            }
            let expanded = expansion.result;

            // A recipe line may expand to several lines — e.g. a `define` used as a canned recipe —
            // and each becomes its own command. A backslash-newline is a continuation, not a split.
            for line in split_recipe_lines(&expanded) {
                // Per-line modifiers (from a `define`'s own lines, or an expanded `$(Q)`) combine
                // with the line-level ones parsed from the raw recipe line above.
                let (per_silent, per_ignore, per_force, command) = strip_command_modifiers(&line);
                let silent = line_silent || per_silent;
                let ignore_errors = line_ignore || per_ignore;
                let force = line_force || per_force;

                // A recipe consisting only of modifiers (e.g. `@`) is a no-op.
                if command.is_empty() {
                    continue;
                }

                // Question mode (-q): a `+` line still runs, but the first non-`+` line means the
                // target is out of date — abort the recipe without echoing or running it.
                if makefile.opts.question && !force {
                    return Ok(RecipeOutcome {
                        commands_run,
                        needs_rebuild: true,
                    });
                }

                // A real command line: it will be echoed and/or run, so it counts as work done.
                commands_run += 1;

                // Echo the command to stdout, unless silenced. `--just-print` always echoes.
                if (!silent && !silent_target) || makefile.opts.just_print {
                    println!("{command}");
                }

                // Under `--just-print` we only echo and move on, except `+` commands always run.
                if makefile.opts.just_print && !force {
                    continue;
                }

                // Execute the recipe line.
                let mut cmd = Command::new(&shell);
                cmd.args(&shell_flags)
                    .arg(command)
                    .envs(env_adds.iter().map(|(k, v)| (k, v)));
                for name in &env_removes {
                    cmd.env_remove(name);
                }
                let res = cmd
                    .status()
                    .map_err(|e| MakeError::new(&e.to_string(), self.context.clone()))?;

                // A non-zero exit (or a signal) is a failure. When the error is ignored (`-` prefix,
                // `.IGNORE`, or `-i`) gmake prints `<prog>: [file:N: target] <reason> (ignored)` to
                // stderr and carries on; otherwise the build aborts.
                if !matches!(res.code(), Some(0)) {
                    let ignored = ignore_errors || ignore_target || makefile.opts.ignore_errors;
                    if ignored {
                        // Suppressed only when *globally* silent (`-s`/`.SILENT:` with no args); a
                        // target-specific `.SILENT` or a per-line `@` does not suppress it.
                        if !makefile.is_silent_all() {
                            let mut ctx = self.context.clone();
                            ctx.line_index = Some(recipe_line);
                            let loc = ctx
                                .label()
                                .map(|l| format!("[{l}: {target}]"))
                                .unwrap_or_else(|| format!("[{target}]"));
                            makefile.logger.write(format!(
                                "{}: {loc} {} (ignored)",
                                makefile.logger.prefix(),
                                exit_reason(&res),
                            ));
                        }
                    } else if makefile.opts.question {
                        // Under -q, a failed forced (`+`) command means "out of date" (exit 1), not
                        // a build error (exit 2). Abort the recipe.
                        return Ok(RecipeOutcome {
                            commands_run,
                            needs_rebuild: true,
                        });
                    } else {
                        self.remove_target_on_error(makefile, target, pattern_build, mtime_before);
                        let msg = match res.code() {
                            Some(code) => format!("Failed with code {code}."),
                            None => "Killed.".to_string(),
                        };
                        // Report the failing recipe line (not the rule header), like gmake.
                        let mut ctx = self.context.clone();
                        ctx.line_index = Some(recipe_line);
                        ctx.content = Some(raw.clone());
                        return Err(MakeError::new(&msg, ctx));
                    }
                }
            }
        }

        // Touch the out-of-date target (`-t`), unless it is phony, has no recipe, or its recipe was
        // entirely `+` lines. The echo is gated only on the global `-s`; `--just-print` echoes but
        // does not touch. Only reached for out-of-date targets, matching gmake's touch condition.
        if touch_mode
            && self.has_recipe
            && !makefile.is_phony(target)
            && (touch_saw_non_plus || !touch_saw_plus)
        {
            if !makefile.opts.silent {
                println!("touch {target}");
            }
            commands_run += 1;
            if !makefile.opts.just_print {
                touch_file(target).map_err(|e| {
                    MakeError::new(&format!("touch: {target}: {e}"), self.context.clone())
                })?;
            }
        }

        Ok(RecipeOutcome {
            commands_run,
            needs_rebuild,
        })
    }

    /// Under `.DELETE_ON_ERROR:`, delete `target` if a failing recipe created or modified it (so a
    /// partial/corrupt file is not left looking up to date). `.PRECIOUS` and phony targets are
    /// exempt. Matches GNU make, which only deletes a target the recipe actually touched.
    /// `pattern_build` (an implicit pattern-rule build) lets `.PRECIOUS` pattern entries (e.g.
    /// `%.o`) protect the target — gmake honors those only for implicitly-built targets, not for
    /// explicit or static-pattern rules.
    fn remove_target_on_error<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        pattern_build: bool,
        mtime_before: Option<std::time::SystemTime>,
    ) {
        if !makefile.delete_on_error()
            || makefile.is_precious(target, pattern_build)
            || makefile.is_phony(target)
        {
            return;
        }
        let mtime_after = std::fs::metadata(target)
            .ok()
            .and_then(|m| m.modified().ok());
        let modified = match (mtime_before, mtime_after) {
            (None, Some(_)) => true,                        // created by the recipe
            (Some(before), Some(after)) => after != before, // modified by the recipe
            _ => false,                                     // absent, or untouched
        };
        if modified {
            makefile
                .logger
                .info(&format!("Deleting file '{target}'"), None);
            let _ = std::fs::remove_file(target);
        }
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

        // Resolve each prerequisite through `VPATH`/`vpath`, so automatic variables use the path
        // where the file was actually found (falling back to the plain name).
        let resolved: Vec<String> = self
            .prerequisites
            .iter()
            .map(|p| makefile.find_file(p).unwrap_or_else(|| p.clone()))
            .collect();

        // `$<`: the first prerequisite.
        let first = resolved.first().map(String::as_str).unwrap_or("");
        let _ = vars.set("<", first, false);

        // `$^`: all prerequisites, de-duplicated with order preserved.
        let mut seen = HashSet::new();
        let deduped = resolved
            .iter()
            .filter(|p| seen.insert(p.as_str()))
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        let _ = vars.set("^", deduped.as_str(), false);

        // `$+`: all prerequisites, keeping duplicates.
        let all = resolved.join(" ");
        let _ = vars.set("+", all.as_str(), false);

        // `$?`: prerequisites newer than the target, de-duplicated. If the target is absent, every
        // prerequisite counts; if the target exists, a prerequisite counts when its file is newer
        // or when it has no file (phony / just-rebuilt prerequisites are treated as newer).
        let target_mtime = makefile.get_mtime(target);
        let mut seen_newer = HashSet::new();
        let newer = resolved
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

        // Directory/file parts of `$@`, `$<`, and `$*`.
        let (at_dir, at_file) = split_dir_file(target);
        let _ = vars.set("@D", at_dir.as_str(), false);
        let _ = vars.set("@F", at_file.as_str(), false);
        let (lt_dir, lt_file) = split_dir_file(first);
        let _ = vars.set("<D", lt_dir.as_str(), false);
        let _ = vars.set("<F", lt_file.as_str(), false);
        let (star_dir, star_file) = split_dir_file(stem);
        let _ = vars.set("*D", star_dir.as_str(), false);
        let _ = vars.set("*F", star_file.as_str(), false);
    }
}

/// A copy of `rule` scoped to a single `target`, so each target of a multi-target rule is stored
/// (and later merged) independently, matching GNU make.
fn single_target(rule: &Rule, target: &str) -> Rule {
    Rule {
        targets: vec![target.to_string()],
        ..rule.clone()
    }
}

/// Create `path` if missing (empty) or bump its mtime to now, for `-t` (touch mode).
fn touch_file(path: &str) -> std::io::Result<()> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.set_modified(SystemTime::now())
}

/// gmake's reason text for a failed recipe command, used in the `(ignored)` diagnostic: `Error N`
/// for a non-zero exit code, or the signal name for a signal-terminated command.
fn exit_reason(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match status.code() {
        Some(code) => format!("Error {code}"),
        None => match status.signal() {
            Some(2) => "Interrupt".to_string(),
            Some(6) => "Aborted (core dumped)".to_string(),
            Some(9) => "Killed".to_string(),
            Some(11) => "Segmentation fault (core dumped)".to_string(),
            Some(15) => "Terminated".to_string(),
            Some(n) => format!("Signal {n}"),
            None => "Terminated".to_string(),
        },
    }
}

/// Strip leading command modifiers (`@` silence, `-` ignore errors, `+` force) from a recipe line.
/// They may be combined in any order and separated by whitespace (e.g. `@-` or `- @`). Returns the
/// three flags and the remaining command text.
fn strip_command_modifiers(s: &str) -> (bool, bool, bool, &str) {
    let (mut silent, mut ignore_errors, mut force) = (false, false, false);
    let mut rest = s;
    loop {
        rest = rest.trim_start();
        match rest.chars().next() {
            Some('@') => silent = true,
            Some('-') => ignore_errors = true,
            Some('+') => force = true,
            _ => break,
        }
        rest = &rest[1..];
    }
    (silent, ignore_errors, force, rest)
}

/// Split an expanded recipe into individual command lines. A newline preceded by an odd number of
/// backslashes is a continuation and is kept within the command (the shell handles it); any other
/// newline separates commands (as with a `define` used as a canned recipe).
fn split_recipe_lines(expanded: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for c in expanded.chars() {
        if c == '\n' {
            let backslashes = current.chars().rev().take_while(|&b| b == '\\').count();
            if backslashes % 2 == 1 {
                current.push('\n');
            } else {
                lines.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

/// Split a path into its directory part (`.` when there is no slash, trailing slash removed) and
/// its file part, matching make's `$(@D)`/`$(@F)`.
fn split_dir_file(path: &str) -> (String, String) {
    match path.rfind('/') {
        Some(i) => (path[..i].to_string(), path[i + 1..].to_string()),
        None => (".".to_string(), path.to_string()),
    }
}

/// Build a `.SECONDEXPANSION:` scope: a copy of `scope` with only the automatic variables that are
/// defined during prerequisite expansion — `$@`, `$*`, and their `D`/`F` parts. `$<`/`$^`/`$?`/`$+`/
/// `$|` are intentionally left empty (they are not known while the prerequisite list is computed).
fn second_expansion_scope(scope: &Vars, target: &str, stem: &str) -> Vars {
    let mut vars = scope.clone();
    let _ = vars.set("@", target, false);
    let _ = vars.set("*", stem, false);
    let (at_dir, at_file) = split_dir_file(target);
    let _ = vars.set("@D", at_dir.as_str(), false);
    let _ = vars.set("@F", at_file.as_str(), false);
    let (star_dir, star_file) = split_dir_file(stem);
    let _ = vars.set("*D", star_dir.as_str(), false);
    let _ = vars.set("*F", star_file.as_str(), false);
    vars
}

/// Second-expand each deferred prerequisite line (`.SECONDEXPANSION:`), then word-split and glob,
/// producing the concrete prerequisite list.
fn expand_deferred_prereqs(raws: &[String], scope: &Vars) -> Result<Vec<String>, MakeError> {
    let mut out = Vec::new();
    for raw in raws {
        let expanded = expand_collect(raw, scope)
            .map_err(|e| MakeError::new(&e, Context::new()))?
            .result;
        out.extend(super::glob_words(
            expanded.split_whitespace().map(str::to_string),
        ));
    }
    Ok(out)
}

/// Produce a concrete copy of a `.SECONDEXPANSION:` rule with its prerequisites resolved (see
/// [`expand_deferred_prereqs`]) so the rest of the build treats it like any other rule.
fn resolve_second_expansion(rule: &Rule, target: &str, scope: &Vars) -> Result<Rule, MakeError> {
    let se_scope = second_expansion_scope(scope, target, &rule.stem);
    let mut resolved = rule.clone();
    resolved.prerequisites = expand_deferred_prereqs(&rule.prerequisites, &se_scope)?;
    resolved.order_only = expand_deferred_prereqs(&rule.order_only, &se_scope)?;
    resolved.second_expand = false;
    Ok(resolved)
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
    /// Source line index of each recipe entry (parallel to `recipe`), for failing-recipe location.
    recipe_lines: Vec<usize>,
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

    /// The concrete prerequisites for a given `stem` (only the first `%` per pattern is the stem).
    fn prerequisites(&self, stem: &str) -> Vec<String> {
        self.prereq_patterns
            .iter()
            .map(|p| p.replacen('%', stem, 1))
            .collect()
    }

    /// The concrete order-only prerequisites for a given `stem`.
    fn order_only(&self, stem: &str) -> Vec<String> {
        self.order_only_patterns
            .iter()
            .map(|p| p.replacen('%', stem, 1))
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

    /// Files named as a prerequisite in some explicit rule. Such a file is never an intermediate
    /// (GNU make only invents/deletes files that appear solely via implicit-rule chaining).
    mentioned: HashSet<String>,
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
            mentioned: HashSet::new(),
        }
    }

    /// Append the built-in implicit pattern rules (unless a user rule already defines the same
    /// pattern). Call after parsing so user rules are matched first.
    pub fn register_builtins(&mut self) {
        for (target, prereqs, recipe) in BUILTIN_RULES {
            let pct = target.find('%').expect("built-in target contains %");
            let prereq_patterns: Vec<String> = prereqs.iter().map(|s| s.to_string()).collect();
            let already_defined = self.pattern_rules.iter().any(|p| {
                p.target_prefix == target[..pct]
                    && p.target_suffix == target[pct + 1..]
                    && p.prereq_patterns == prereq_patterns
            });
            if !already_defined {
                self.pattern_rules.push(PatternRule {
                    target_prefix: target[..pct].to_string(),
                    target_suffix: target[pct + 1..].to_string(),
                    prereq_patterns,
                    order_only_patterns: vec![],
                    recipe: vec![recipe.to_string()],
                    recipe_lines: vec![0],
                    context: Context::new(),
                });
            }
        }
    }

    /// Insert a rule, update the `by_target` hashmap, and validate the rule.
    pub fn insert<L: Logger>(&mut self, rule: Rule, logger: &L) -> Result<(), MakeError> {
        // A rule with a `%` target is a pattern rule, stored separately from exact-match rules.
        if rule.targets.iter().any(|t| t.contains('%')) {
            for target in &rule.targets {
                if let Some(pct) = target.find('%') {
                    let new = PatternRule {
                        target_prefix: target[..pct].to_string(),
                        target_suffix: target[pct + 1..].to_string(),
                        prereq_patterns: rule.prerequisites.clone(),
                        order_only_patterns: rule.order_only.clone(),
                        recipe: rule.recipe.clone(),
                        recipe_lines: rule.recipe_lines.clone(),
                        context: rule.context.clone(),
                    };

                    // A later definition of the same pattern (target + prerequisites) replaces the
                    // earlier one. A definition with no recipe cancels the pattern (BSD make style):
                    // it is stored as a recipe-less rule that `match_pattern_rule`/`can_be_made`
                    // skip, and it suppresses the matching built-in (via `register_builtins`).
                    match self.pattern_rules.iter_mut().find(|p| {
                        p.target_prefix == new.target_prefix
                            && p.target_suffix == new.target_suffix
                            && p.prereq_patterns == new.prereq_patterns
                    }) {
                        Some(existing) => *existing = new,
                        None => self.pattern_rules.push(new),
                    }
                }
            }
            return Ok(());
        }

        // Record explicit prerequisites so such files are never treated as invented intermediates.
        // Deferred (`.SECONDEXPANSION:`) prerequisites are still un-expanded here, so skip them.
        if !rule.second_expand {
            self.mentioned.extend(rule.prerequisites.iter().cloned());
            self.mentioned.extend(rule.order_only.iter().cloned());
        }

        // Each target of a rule is stored as its own rule instance, since GNU make treats the
        // targets of a multi-target rule (`a b: x`) independently: a later stanza extending one
        // target must not affect its siblings. Several single-colon stanzas for one target are then
        // *merged* — prerequisites are combined with the recipe-bearing stanza's first, then the
        // others in definition order; a later recipe overrides an earlier one (with a warning).
        // Double-colon rules stay independent.
        for target in &rule.targets {
            let Some(first) = self.by_target.get(target).map(|v| v[0]) else {
                let index = self.rules.len();
                self.rules.push(single_target(&rule, target));
                self.by_target.insert(target.to_owned(), vec![index]);
                continue;
            };

            if self.rules[first].double_colon != rule.double_colon {
                return Err(MakeError::new(
                    "Cannot define rules using `:` and `::` on the same target.",
                    rule.context.clone(),
                ));
            }

            if rule.double_colon {
                let index = self.rules.len();
                self.rules.push(single_target(&rule, target));
                self.by_target.get_mut(target).unwrap().push(index);
                continue;
            }

            let old_recipe = !self.rules[first].recipe.is_empty();
            let old_context = self.rules[first].context.clone();
            let existing = &mut self.rules[first];
            // A merged rule needs a second expansion if any of its stanzas did (each stanza's stored
            // prerequisites re-expand independently at build time).
            existing.second_expand |= rule.second_expand;
            if rule.has_recipe {
                // This stanza carries the recipe: its prerequisites go in front, and it supplies the
                // recipe (even an empty one, which clears the old) plus its stem/context (so a static
                // pattern stanza overriding a plain one keeps its `$*`).
                existing
                    .prerequisites
                    .splice(0..0, rule.prerequisites.iter().cloned());
                existing
                    .order_only
                    .splice(0..0, rule.order_only.iter().cloned());
                existing.has_recipe = true;
                if old_recipe {
                    logger.warn(
                        &format!("overriding recipe for target '{target}'"),
                        Some(&rule.context),
                    );
                    logger.warn(
                        &format!("ignoring old recipe for target '{target}'"),
                        Some(&old_context),
                    );
                }
                existing.recipe = rule.recipe.clone();
                existing.recipe_lines = rule.recipe_lines.clone();
                existing.stem = rule.stem.clone();
                existing.context = rule.context.clone();
            } else {
                existing
                    .prerequisites
                    .extend(rule.prerequisites.iter().cloned());
                existing.order_only.extend(rule.order_only.iter().cloned());
            }
        }

        Ok(())
    }

    /// Execute the given `targets` in order, sharing build state so each is built at most once and
    /// circular dependencies are detected rather than recursed into forever.
    pub fn execute<L>(&self, makefile: &Makefile<L>, targets: &[String]) -> Result<bool, MakeError>
    where
        L: Logger,
    {
        let mut state = ExecState {
            goals: targets.iter().cloned().collect(),
            ..Default::default()
        };
        // A build error (without `--keep-going`) is held until after the intermediate cleanup below,
        // since GNU make deletes invented intermediates even when the build fails.
        let mut build_error = None;
        for target in targets {
            // Top-level targets have no parent edge (they cannot be circular on first entry). The
            // initial variable scope is the makefile's global variables.
            let recipes_before = state.recipes_run;
            if let Err(e) = self.execute_target(makefile, target, "", &makefile.vars, &mut state) {
                build_error = Some(e);
                break;
            }

            // If nothing ran for this goal, report it: "up to date" when the goal carries a recipe
            // of its own, else "Nothing to be done" (matching GNU make). Question mode (`-q`) prints
            // no summary — the exit code carries the answer.
            if !makefile.opts.question
                && state.recipes_run == recipes_before
                && !state.failed.contains(target)
            {
                let message = if self.has_recipe(makefile, target) {
                    format!("'{target}' is up to date.")
                } else {
                    format!("Nothing to be done for '{target}'.")
                };
                makefile.logger.info(&message, None);
            }
        }

        // Delete intermediate files this run invented, unless preserved by `.SECONDARY`/`.PRECIOUS`
        // (matching GNU make, which keeps the source tree clean of chained-rule byproducts). Only
        // files the recipe actually created are removed, and they are reported in one `rm` line.
        // Under `--just-print`, the recipe did not create the files, but the `rm` is still shown.
        let to_delete: Vec<&String> = state
            .invented
            .iter()
            .filter(|f| {
                !makefile.is_secondary(f)
                    && (makefile.opts.just_print || std::path::Path::new(f).exists())
            })
            .collect();
        if !to_delete.is_empty() {
            let names = to_delete
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            makefile.logger.info(&format!("rm {names}"), None);
            if !makefile.opts.just_print {
                for file in to_delete {
                    let _ = std::fs::remove_file(file);
                }
            }
        }

        // A build error (without `--keep-going`) is propagated now, after the cleanup above.
        if let Some(e) = build_error {
            return Err(e);
        }

        // Under `--keep-going` failures are recorded rather than propagated, so report them now.
        if !state.failed.is_empty() {
            return Err(MakeError::new(
                "Some targets were not remade because of errors.",
                Context::new(),
            ));
        }
        Ok(state.needs_rebuild)
    }

    /// Build a single target, guarding against re-building (memoization) and circular dependencies.
    /// `parent` is the target that required this one (for the circular-dependency diagnostic).
    fn execute_target<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        parent: &str,
        scope: &Vars,
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

        let result = self.build_target(makefile, target, scope, state);

        state.in_progress.remove(target);
        match result {
            Ok(()) => {
                state.done.insert(target.to_string());
                Ok(())
            }
            // Under `--keep-going`, report the failure but continue building unrelated targets.
            Err(e) if makefile.opts.keep_going => {
                makefile.logger.error(&e.msg, Some(&e.context));
                state.failed.insert(target.to_string());
                state.done.insert(target.to_string());
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// The core build logic for one target: resolve prerequisites, then run recipes if out of date.
    fn build_target<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        scope: &Vars,
        state: &mut ExecState,
    ) -> Result<(), MakeError> {
        // Extend the variable scope with this target's target-specific variables (`target: VAR =
        // value`); they stay in scope while this target and its prerequisites are built. Only clone
        // when the target actually has such variables.
        let mut extended;
        let scope: &Vars = if makefile.has_target_vars(target) {
            extended = scope.clone();
            makefile.apply_target_vars(&mut extended, target);
            &extended
        } else {
            scope
        };

        // Whether the target already existed, so a `.INTERMEDIATE`-marked file its own explicit rule
        // creates can be recognized as invented (and deleted afterward).
        let existed_before = makefile.get_mtime(target).is_some();

        let rule_indices = match self.by_target.get(target) {
            Some(indices) => indices,
            None => {
                // No explicit rule. Try pattern rules first (they can rebuild an existing target
                // when a prerequisite is newer).
                if let Some((idx, stem)) = self.match_pattern_rule(makefile, target) {
                    let borrowed = BorrowedPrereqs::default();
                    return self
                        .build_from_pattern(makefile, target, idx, &stem, &borrowed, scope, state);
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
                        recipe_lines: vec![0; default.len()],
                        recipe: default.to_vec(),
                        has_recipe: true,
                        stem: String::new(),
                        target_pattern: None,
                        second_expand: false,
                        context: Context::new(),
                        double_colon: false,
                    };
                    let outcome = rule.execute(makefile, target, false, scope)?;
                    state.recipes_run += outcome.commands_run;
                    state.needs_rebuild |= outcome.needs_rebuild;
                    return Ok(());
                }
                return Err(MakeError::new(
                    &format!("No rule to make target '{target}'."),
                    Context::new(),
                ));
            }
        };

        // Old files have their rules ignored. This reports its own status, so mark the target
        // reported (`recipes_run`) to suppress the generic top-level summary.
        if makefile.opts.old_file.iter().any(|f| f == target) {
            makefile
                .logger
                .info(&format!("Target '{target}' is up to date (old)."), None);
            state.recipes_run += 1;
            return Ok(());
        }

        // If the explicit rule(s) supply prerequisites but no recipe, GNU make lets the target
        // borrow a matching pattern/built-in rule's recipe, merging the explicit prerequisites in
        // after the pattern-derived ones. Implicit rules never apply to a phony target.
        let has_recipe = rule_indices.iter().any(|i| self.rules[*i].has_recipe);
        if !has_recipe
            && !makefile.is_phony(target)
            && let Some((idx, stem)) = self.match_pattern_rule(makefile, target)
        {
            let borrowed = BorrowedPrereqs {
                normal: rule_indices
                    .iter()
                    .flat_map(|i| self.rules[*i].prerequisites.clone())
                    .collect(),
                order_only: rule_indices
                    .iter()
                    .flat_map(|i| self.rules[*i].order_only.clone())
                    .collect(),
            };
            return self.build_from_pattern(makefile, target, idx, &stem, &borrowed, scope, state);
        }

        for i in rule_indices {
            let stored = &self.rules[i.to_owned()];
            // `.SECONDEXPANSION:`: resolve the deferred prerequisites now, with `$@`/`$*` in scope,
            // into a concrete rule the rest of the build uses unchanged.
            let resolved;
            let rule: &Rule = if stored.second_expand {
                resolved = resolve_second_expansion(stored, target, scope)?;
                &resolved
            } else {
                stored
            };
            // Re-read the target mtime per rule so a double-colon rule sees files written by an
            // earlier rule for the same target.
            let target_mtime_opt = makefile.get_mtime(target);
            let mut should_execute = makefile.opts.always_make;
            let mut prereq_failed = false;

            // Check (and possibly execute) prereqs.
            for prereq in &rule.prerequisites {
                // Check if prereq exists unless `always_make`.
                if makefile.opts.always_make {
                    self.execute_target(makefile, prereq, target, scope, state)?;
                } else {
                    match makefile.get_mtime(prereq) {
                        None => {
                            // Prereq doesn't exist, so make it. By definition, it's more up-to-date
                            // than the target.
                            self.execute_target(makefile, prereq, target, scope, state)?;
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

                // Under `--keep-going`, a failed prerequisite is recorded rather than propagated,
                // so check for it here — this target can't be built if any prerequisite failed.
                if state.failed.contains(prereq) {
                    prereq_failed = true;
                }
            }

            // Order-only prerequisites are made if missing but never force a rebuild.
            self.make_order_only(makefile, &rule.order_only, target, scope, state)?;
            if rule.order_only.iter().any(|p| state.failed.contains(p)) {
                prereq_failed = true;
            }

            if prereq_failed {
                return Err(MakeError::new(
                    &format!("Target '{target}' not remade because of errors."),
                    Context::new(),
                ));
            }

            if target_mtime_opt.is_none() || should_execute {
                let outcome = rule.execute(makefile, target, false, scope)?;
                state.recipes_run += outcome.commands_run;
                state.needs_rebuild |= outcome.needs_rebuild;
            }
        }

        // A `.INTERMEDIATE`-marked file that its own explicit rule just created (absent before) is an
        // invented intermediate to delete after the build.
        if !existed_before
            && self.is_intermediate(makefile, target, state)
            && makefile.get_mtime(target).is_some()
        {
            state.invented.push(target.to_string());
        }

        Ok(())
    }

    /// Whether `target` "has commands" for the up-to-date vs Nothing-to-be-done summary (used when a
    /// goal needed no work). Like GNU make: a phony target is never up to date; otherwise the target
    /// has commands if it has an explicit rule with a recipe section (even an empty one), a
    /// recipe-less explicit rule that can borrow a matching pattern recipe, or (with no explicit
    /// rule) an applicable implicit rule with a recipe, else a non-empty `.DEFAULT`.
    fn has_recipe<L: Logger>(&self, makefile: &Makefile<L>, target: &str) -> bool {
        if makefile.is_phony(target) {
            return false;
        }
        let borrowable = || {
            self.match_pattern_rule(makefile, target)
                .is_some_and(|(idx, _)| !self.pattern_rules[idx].recipe.is_empty())
        };
        // An explicit rule decides on its own (gmake does not fall back to `.DEFAULT` once the
        // target has a rule). A double-colon target's wording keys on its first entry; a normal
        // target counts a recipe from any entry, or borrows a pattern recipe if it has none.
        if let Some(indices) = self.by_target.get(target) {
            if self.rules[indices[0]].double_colon {
                return self.rules[indices[0]].has_recipe || borrowable();
            }
            return indices.iter().any(|i| self.rules[*i].has_recipe) || borrowable();
        }
        // No explicit rule: an applicable implicit (pattern/built-in) rule with a recipe, else
        // `.DEFAULT`, whose recipe (if any) supplies the target's commands.
        borrowable() || !makefile.default_recipe().is_empty()
    }

    /// Find the first pattern rule whose target pattern matches `target` and whose (stem-expanded)
    /// prerequisites can all be made. Returns the rule's index and the matched stem.
    fn match_pattern_rule<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
    ) -> Option<(usize, String)> {
        for (idx, prule) in self.pattern_rules.iter().enumerate() {
            // A recipe-less pattern rule is a cancellation, not a buildable rule.
            if prule.recipe.is_empty() {
                continue;
            }
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
    /// rules, e.g. `%.o: %.c` with `%.c: %.y`). `depth` bounds the chain to avoid infinite
    /// recursion.
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
        self.pattern_rules.iter().any(|prule| {
            // A recipe-less pattern rule is a cancellation, so it produces nothing.
            if prule.recipe.is_empty() {
                return false;
            }
            // Skip match-anything rules (target `%`) when chaining: they match every name and would
            // grow prerequisites forever (`x.c.c.c…`). GNU make also does not use match-anything
            // rules to build the prerequisites of another implicit rule.
            if prule.target_prefix.is_empty() && prule.target_suffix.is_empty() {
                return false;
            }
            match prule.match_stem(name) {
                Some(stem) => prule
                    .prerequisites(stem)
                    .iter()
                    .all(|p| self.can_be_made(makefile, p, depth - 1)),
                None => false,
            }
        })
    }

    /// Whether `target` is an intermediate file: a non-goal, non-phony file made only by an implicit
    /// rule (a chain link like `foo.c` in `foo.y` -> `foo.c` -> `foo.o`), or one forced intermediate
    /// with `.INTERMEDIATE`. Intermediates are not rebuilt merely for being absent, and an invented
    /// one is deleted after the build.
    fn is_intermediate<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        state: &ExecState,
    ) -> bool {
        if state.goals.contains(target) || makefile.is_phony(target) {
            return false;
        }
        if makefile.is_intermediate_marked(target) {
            return true;
        }
        // A file named as an explicit prerequisite (or with its own explicit rule) is a real file,
        // not an invented intermediate.
        !self.mentioned.contains(target)
            && !self.by_target.contains_key(target)
            && self.match_pattern_rule(makefile, target).is_some()
    }

    /// The mtime a (possibly missing) file effectively has for out-of-date checks. An existing file
    /// uses its real mtime; a missing file buildable by a pattern rule uses the newest mtime among
    /// its (recursively resolved) prerequisites — so a missing intermediate does not force a rebuild
    /// unless its own sources are newer than the target that needs it.
    fn effective_mtime<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        depth: usize,
    ) -> Option<SystemTime> {
        if let Some(mtime) = makefile.get_mtime(target) {
            return Some(mtime);
        }
        if depth == 0 {
            return None;
        }
        let (idx, stem) = self.match_pattern_rule(makefile, target)?;
        self.pattern_rules[idx]
            .prerequisites(&stem)
            .iter()
            .filter_map(|p| self.effective_mtime(makefile, p, depth - 1))
            .max()
    }

    /// Build `target` using the pattern rule at `idx` with the given `stem`. `borrowed` holds
    /// prerequisites from a recipe-less explicit rule for `target`; they are appended after the
    /// pattern-derived ones, so `$<` stays the pattern's first prerequisite and `$^` lists the
    /// pattern prerequisite(s) then the explicit ones (matching GNU make).
    #[allow(clippy::too_many_arguments)]
    fn build_from_pattern<L: Logger>(
        &self,
        makefile: &Makefile<L>,
        target: &str,
        idx: usize,
        stem: &str,
        borrowed: &BorrowedPrereqs,
        scope: &Vars,
        state: &mut ExecState,
    ) -> Result<(), MakeError> {
        let prule = &self.pattern_rules[idx];
        let mut prereqs = prule.prerequisites(stem);
        prereqs.extend(borrowed.normal.iter().cloned());
        let mut order_only = prule.order_only(stem);
        order_only.extend(borrowed.order_only.iter().cloned());
        let target_mtime = makefile.get_mtime(target);

        // Pass 1: decide whether the target is out of date, without building anything yet. A missing
        // prerequisite that is an intermediate file is judged by its effective mtime (its own
        // sources) rather than treated as infinitely new, so a missing intermediate does not force a
        // rebuild when the final target is already up to date.
        let mut should_execute = makefile.opts.always_make;
        for prereq in &prereqs {
            let prereq_mtime = if makefile.get_mtime(prereq).is_some() {
                makefile.get_mtime(prereq)
            } else if self.is_intermediate(makefile, prereq, state) {
                self.effective_mtime(makefile, prereq, PATTERN_CHAIN_LIMIT)
            } else {
                None
            };
            match prereq_mtime {
                None => should_execute = true,
                Some(m) => {
                    if target_mtime.is_none_or(|tm| m > tm) {
                        should_execute = true;
                    }
                }
            }
        }

        // Order-only prerequisites are always made if missing (but never force a rebuild).
        self.make_order_only(makefile, &order_only, target, scope, state)?;

        if target_mtime.is_none() || should_execute {
            // Build the normal prerequisites (including any intermediate now actually needed).
            for prereq in &prereqs {
                self.execute_target(makefile, prereq, target, scope, state)?;
            }

            // Under `--keep-going`, a failed prerequisite is recorded rather than propagated, so
            // this target must not be built if any (normal or order-only) prerequisite failed.
            if prereqs
                .iter()
                .chain(&order_only)
                .any(|p| state.failed.contains(p))
            {
                return Err(MakeError::new(
                    &format!("Target '{target}' not remade because of errors."),
                    Context::new(),
                ));
            }

            let rule = Rule {
                targets: vec![target.to_string()],
                prerequisites: prereqs,
                order_only,
                has_recipe: !prule.recipe.is_empty(),
                recipe: prule.recipe.clone(),
                recipe_lines: prule.recipe_lines.clone(),
                stem: stem.to_string(),
                target_pattern: None,
                second_expand: false,
                context: prule.context.clone(),
                double_colon: false,
            };
            let outcome = rule.execute(makefile, target, true, scope)?;
            state.recipes_run += outcome.commands_run;
            state.needs_rebuild |= outcome.needs_rebuild;

            // A file just built by an implicit rule that was absent beforehand and is only a chain
            // link (not a goal) is an invented intermediate; record it for deletion after the build.
            if target_mtime.is_none() && self.is_intermediate(makefile, target, state) {
                state.invented.push(target.to_string());
            }
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
        scope: &Vars,
        state: &mut ExecState,
    ) -> Result<(), MakeError> {
        for prereq in order_only {
            self.execute_target(makefile, prereq, parent, scope, state)?;
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
    /// Targets that failed to build (only used under `--keep-going`, which continues past errors).
    failed: HashSet<String>,
    /// Count of rules whose (non-empty) recipe ran, so a goal that ran nothing can report "up to
    /// date" / "Nothing to be done".
    recipes_run: usize,
    /// Under `-q`, whether any built target was found out of date (drives the exit-1 result).
    needs_rebuild: bool,
    /// The top-level goals; a goal is never treated as an intermediate file.
    goals: HashSet<String>,
    /// Intermediate files invented this run (built by an implicit rule as a non-goal chain link,
    /// and absent beforehand). Deleted after the build unless `.SECONDARY`/`.PRECIOUS`.
    invented: Vec<String>,
}
