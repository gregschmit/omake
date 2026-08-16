//! Implementation of variable expansion.

use std::process::Command;

use crate::vars::{Origin, Vars};

/// Mutable state carried through a single top-level expansion.
#[derive(Default)]
struct Expander {
    /// Names of recursive variables currently being expanded — the cycle guard that rejects a
    /// variable referencing itself (directly or through a chain) instead of overflowing the stack.
    expanding: Vec<String>,
    /// Text collected by `$(eval ...)`, to be parsed as makefile syntax by the caller once the
    /// expansion (and its borrow of the variables) finishes.
    evals: Vec<String>,
}

/// A stack frame used to track the previous buffer when expanding potentially nested expressions
/// (i.e., either `$()` or `${}`). Single variable expansions (e.g., `$X`) are handled inline
/// without creating a frame since they cannot possibly have nested expressions.
struct Frame {
    pub previous_buffer: String,
    /// Which character opened this stack frame (parenthesis or brace)?
    pub opening_delimiter: char,
    /// Byte offset into the original input where this expression's content starts (just after the
    /// opening delimiter). Used to recover the *raw*, unexpanded text — needed so a function splits
    /// its arguments before they are expanded (e.g. `$(subst $(comma),X,a,b)`).
    pub raw_start: usize,
    /// Nesting depth of matching literal `(`/`{` inside this expression, so a later literal
    /// close is not mistaken for the end of the expression (`$(subst (a,b),Z,text)`). In
    /// `raw_mode` it also counts nested `$(...)` so the matching close is found without expanding.
    pub paren_depth: u32,
    /// Set once this frame is known to be a control-function call: its argument text is collected
    /// verbatim (nothing is expanded) until the matching close, so lazy branches and side effects
    /// behave correctly.
    pub raw_mode: bool,
    /// Whether the name-so-far has been tested for being a control function (tested once, at the
    /// whitespace ending the name).
    pub checked_name: bool,
}

/// Run variable expansion on an input string, given a collection of `vars`.
///
/// The goal here is to be `O(n)`. This works by iterating over the input string and storing plain
/// characters into a buffer until we hit either:
///  1. A simple variable expansion (e.g., `$X`), where we just evaluate it against `vars` inline
///     since there could not possibly be any nesting.
///  2. A long variable expansion (e.g., `$(` or `${`), where there could be nested
///     expressions, where we push the current buffer onto a stack, and then continue parsing. When
///     we hit a matching closing delimiter (tracked on the stack frame), we evaluate the buffer,
///     pop the previous buffer off the stack, join it with the evaluated value, and keep going.
pub fn expand(s: &str, vars: &Vars) -> Result<String, String> {
    Ok(expand_collect(s, vars)?.0)
}

/// Like [`expand`], but also returns the text collected by any `$(eval ...)` calls, for the caller
/// (the makefile parser) to evaluate. Plain [`expand`] discards it, so `$(eval)` is a no-op in
/// contexts that cannot process it (e.g. recipe expansion).
pub fn expand_collect(s: &str, vars: &Vars) -> Result<(String, Vec<String>), String> {
    let mut ctx = Expander::default();
    let result = expand_inner(s, vars, &mut ctx)?;
    Ok((result, ctx.evals))
}

/// Inner expansion that carries the [`Expander`] context (recursive-variable cycle guard and the
/// `$(eval)` sink).
// Not a `pop_if`: the raw-mode frame is only popped when its matching close is reached, not on
// every character it consumes.
#[allow(clippy::manual_pop_if)]
fn expand_inner(s: &str, vars: &Vars, ctx: &mut Expander) -> Result<String, String> {
    let mut stack: Vec<Frame> = vec![];
    let mut current_buffer: String = String::with_capacity(s.len());
    let mut hit_variable: bool = false;

    for (i, c) in s.char_indices() {
        // Inside a control function, collect its argument text verbatim (expanding nothing) until
        // the matching close, tracking only delimiter depth. This is what makes lazy branches
        // (`$(if)`) and side-effecting / binding functions (`$(shell)`/`$(foreach)`/`$(call)`/
        // `$(info)`) behave correctly instead of being eagerly evaluated as the scanner passes.
        if stack.last().is_some_and(|f| f.raw_mode) {
            let opener = stack.last().unwrap().opening_delimiter;
            let closer = if opener == '(' { ')' } else { '}' };
            if c == opener {
                stack.last_mut().unwrap().paren_depth += 1;
            } else if c == closer {
                let frame = stack.last_mut().unwrap();
                if frame.paren_depth > 0 {
                    frame.paren_depth -= 1;
                } else {
                    let raw = &s[frame.raw_start..i];
                    let (func, raw_args) = raw
                        .split_once(char::is_whitespace)
                        .expect("raw_mode is only set after a name and whitespace were seen");
                    let result = call_control(func, raw_args, opener, vars, ctx)?
                        .expect("raw_mode is only set for control functions");
                    current_buffer = stack.pop().unwrap().previous_buffer;
                    current_buffer.push_str(&result);
                    hit_variable = false;
                }
            }
            continue;
        }

        // The whitespace ending a function name switches the enclosing frame to raw-accumulation
        // mode when that name is a control function. The check uses the RAW name token, not the
        // (inline-expanded) buffer, so a computed name like `$($(F) …)` is a variable, not a call.
        if (c == ' ' || c == '\t')
            && !current_buffer.trim().is_empty()
            && stack.last().is_some_and(|f| !f.checked_name)
        {
            let top = stack.last_mut().unwrap();
            top.checked_name = true;
            // The name must immediately follow `$(` (no leading whitespace), matching GNU make.
            if is_control_function(&s[top.raw_start..i]) {
                top.raw_mode = true;
                continue;
            }
        }

        match c {
            '$' => {
                hit_variable = !hit_variable;

                // Push a literal `$` if it's the second one (`hit_variable` is `false`)
                if !hit_variable {
                    current_buffer.push(c);
                }
            }
            '(' | '{' => {
                // If we haven't hit a variable, this is a literal delimiter. A literal `(`/`{` that
                // matches the enclosing expression's opener nests one level, so its later close is
                // treated as literal too (e.g. `$(subst (a,b),Z,text)`).
                if !hit_variable {
                    if let Some(frame) = stack.last_mut()
                        && c == frame.opening_delimiter
                    {
                        frame.paren_depth += 1;
                    }
                    current_buffer.push(c);
                    continue;
                }

                // `$(` / `${` opens a frame. `raw_start` is the byte just after the delimiter.
                stack.push(Frame {
                    previous_buffer: current_buffer,
                    opening_delimiter: c,
                    raw_start: i + 1,
                    paren_depth: 0,
                    raw_mode: false,
                    checked_name: false,
                });
                current_buffer = String::new();
                hit_variable = false;
            }
            ')' | '}' => {
                let Some(f) = stack.last() else {
                    current_buffer.push(c);
                    hit_variable = false;
                    continue;
                };
                let matches_closer = (c == '}' && f.opening_delimiter == '{')
                    || (c == ')' && f.opening_delimiter == '(');

                // A close that matches a nested literal open is itself literal.
                if matches_closer && f.paren_depth > 0 {
                    stack.last_mut().unwrap().paren_depth -= 1;
                    current_buffer.push(c);
                    hit_variable = false;
                    continue;
                }
                if !matches_closer {
                    // Wrong delimiter (e.g. `}` inside `$(...)`): a literal char.
                    current_buffer.push(c);
                    hit_variable = false;
                    continue;
                }

                // The expression is terminated. Its raw (unexpanded) text lets a function split its
                // arguments before expanding them.
                let opener = f.opening_delimiter;
                let raw = &s[f.raw_start..i];

                // A text function is `$(name args)`: a known function name (immediately after `$(`)
                // followed by whitespace. Each argument is split from the raw text (commas inside
                // the opener's delimiter are protected) and expanded independently. (Control
                // functions never reach here — raw mode handles them.) Unknown names fall through.
                if let Some((func, raw_args)) = raw.split_once(char::is_whitespace)
                    && let Some(arity) = function_arity(func)
                {
                    let parts = split_function_args(raw_args.trim_start(), arity, opener);
                    // GNU make aborts when a multi-argument function is given too few arguments.
                    if parts.len() < arity {
                        return Err(format!(
                            "insufficient number of arguments ({}) to function '{func}'.",
                            parts.len()
                        ));
                    }
                    let mut args = Vec::with_capacity(arity);
                    for part in parts {
                        args.push(expand_inner(part, vars, ctx)?);
                    }
                    let result = call_function(func, &args)?;
                    current_buffer = stack.pop().unwrap().previous_buffer;
                    current_buffer.push_str(&result);
                    hit_variable = false;
                    continue;
                }

                // A `:` in `${NAME:...}` introduces a GNU substitution reference (`NAME:FROM=TO`,
                // suffix or `%` form) or a BSD modifier chain (`NAME:M...:S/.../.../...`). A `=`
                // marks it a GNU reference (so every GNU makefile keeps working); otherwise BSD
                // modifiers are tried. The name uses the inline-expanded buffer.
                let content = current_buffer.trim();
                let (name, action) = parse_var_action(content);

                let var = vars.get(&name);
                let recursive_result: String;

                // Handle recursive variable expansion, guarding against a cycle.
                let value = if var.recursive {
                    if ctx.expanding.contains(&name) {
                        return Err(format!("Recursive variable '{name}' references itself."));
                    }
                    ctx.expanding.push(name.clone());
                    recursive_result = expand_inner(var.value.as_str(), vars, ctx)?;
                    ctx.expanding.pop();
                    &recursive_result
                } else {
                    &var.value
                };

                // Apply the substitution reference or BSD modifiers to the resolved value.
                let transformed: String;
                let result: &str = match &action {
                    VarAction::None => value,
                    VarAction::Gnu(from, to) => {
                        transformed = substitute_ref(value, from, to);
                        &transformed
                    }
                    VarAction::Bsd(mods) => {
                        transformed = apply_bsd_modifiers(value, mods, vars.is_set(&name));
                        &transformed
                    }
                };

                current_buffer = stack.pop().unwrap().previous_buffer;
                current_buffer.push_str(result);
                hit_variable = false;
            }
            _ => {
                // If we hit the variable indicator, then inline expansion since nesting is
                // impossible.
                if hit_variable {
                    let eval = &vars.get(c.to_string()).value;
                    current_buffer.push_str(eval);
                    hit_variable = false;
                    continue;
                }

                // Otherwise, just push the char.
                current_buffer.push(c);
            }
        }
    }

    // A dangling `$` at the very end of the input (nothing followed it) is a literal `$`.
    if hit_variable {
        current_buffer.push('$');
    }

    // Return current buffer if the stack is empty, else an error.
    match stack.pop() {
        None => Ok(current_buffer),
        Some(frame) => Err(format!(
            "Unclosed variable: {}{}",
            frame.opening_delimiter, frame.previous_buffer
        )),
    }
}

/// The number of comma-separated arguments a function takes, or `None` if `name` is not a known
/// function (so the caller falls back to a variable reference). The last argument absorbs any
/// extra commas.
fn function_arity(name: &str) -> Option<usize> {
    Some(match name {
        "subst" | "patsubst" | "wordlist" => 3,
        "filter" | "filter-out" | "word" | "addprefix" | "addsuffix" | "join" => 2,
        "wildcard" | "strip" | "sort" | "words" | "firstword" | "lastword" | "dir" | "notdir"
        | "basename" | "suffix" | "realpath" | "abspath" | "file" => 1,
        _ => return None,
    })
}

/// Dispatch a make function on its already-split, already-expanded arguments. `name` must be a
/// known function (see `function_arity`). Errors on invalid input (e.g. a bad `word` index).
fn call_function(name: &str, args: &[String]) -> Result<String, String> {
    Ok(match name {
        "word" => return func_word(args),
        "wordlist" => return func_wordlist(args),
        "file" => return func_file(arg(args, 0)),
        "wildcard" => wildcard(arg(args, 0)),
        "realpath" => func_realpath(arg(args, 0)),
        "abspath" => func_abspath(arg(args, 0)),
        "subst" => func_subst(args),
        "patsubst" => func_patsubst(args),
        "strip" => func_strip(arg(args, 0)),
        "sort" => func_sort(arg(args, 0)),
        "filter" => func_filter(args, false),
        "filter-out" => func_filter(args, true),
        "words" => func_words(arg(args, 0)),
        "firstword" => func_firstword(arg(args, 0)),
        "lastword" => func_lastword(arg(args, 0)),
        "dir" => func_dir(arg(args, 0)),
        "notdir" => func_notdir(arg(args, 0)),
        "basename" => func_basename(arg(args, 0)),
        "suffix" => func_suffix(arg(args, 0)),
        "addprefix" => func_addprefix(args),
        "addsuffix" => func_addsuffix(args),
        "join" => func_join(args),
        _ => unreachable!("call_function is only invoked for a known function"),
    })
}

/// Whether `name` is a control function (handled with raw, unexpanded arguments).
fn is_control_function(name: &str) -> bool {
    matches!(
        name,
        "shell"
            | "if"
            | "or"
            | "and"
            | "foreach"
            | "call"
            | "value"
            | "flavor"
            | "origin"
            | "eval"
            | "info"
            | "warning"
            | "error"
    )
}

/// Dispatch a control function — one that needs the raw (unexpanded) arguments plus `vars`, because
/// it expands its arguments lazily, binds variables, or runs a shell. Returns `Ok(None)` if `name`
/// is not a control function (so the caller falls back to text functions / a variable reference).
/// `$(error)` propagates via `Err`; the caller turns it into a located make error.
fn call_control(
    name: &str,
    raw_args: &str,
    open: char,
    vars: &Vars,
    ctx: &mut Expander,
) -> Result<Option<String>, String> {
    let result = match name {
        "shell" => func_shell(raw_args, vars, ctx)?,
        "if" => func_if(raw_args, open, vars, ctx)?,
        "or" => func_or(raw_args, open, vars, ctx)?,
        "and" => func_and(raw_args, open, vars, ctx)?,
        "foreach" => func_foreach(raw_args, open, vars, ctx)?,
        "call" => func_call(raw_args, open, vars, ctx)?,
        "value" => func_value(raw_args, vars, ctx)?,
        "flavor" => func_flavor(raw_args, vars, ctx)?,
        "origin" => func_origin(raw_args, vars, ctx)?,
        "eval" => func_eval(raw_args, vars, ctx)?,
        "info" => func_diagnostic(raw_args, vars, ctx, Stream::Stdout)?,
        "warning" => func_diagnostic(raw_args, vars, ctx, Stream::Stderr)?,
        // `$(error TEXT)` aborts: expand the message and return it as an error.
        "error" => return Err(expand_inner(raw_args.trim_start(), vars, ctx)?),
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// `$(shell COMMAND)`: expand COMMAND, run it through `$(SHELL) $(.SHELLFLAGS)`, and return its
/// stdout with all trailing newlines removed and each remaining newline turned into a space (a CR
/// right before such a newline is dropped; a lone CR is kept). An empty command runs nothing.
///
/// Note: unlike GNU make, this does not set `.SHELLSTATUS` (expansion has only a shared `&Vars`).
fn func_shell(raw_args: &str, vars: &Vars, ctx: &mut Expander) -> Result<String, String> {
    let command = expand_inner(raw_args, vars, ctx)?;
    if command.trim().is_empty() {
        return Ok(String::new());
    }

    let shell = vars.get("SHELL").value.clone();
    let flags: Vec<String> = vars
        .get(".SHELLFLAGS")
        .value
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let output = Command::new(&shell)
        .args(&flags)
        .arg(&command)
        // Capture stdout only; let the command's stderr reach the terminal, like GNU make.
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| e.to_string())?;

    let mut result = String::from_utf8_lossy(&output.stdout).into_owned();
    // A NUL byte truncates the captured output (make treats it as C-string data).
    if let Some(nul) = result.find('\0') {
        result.truncate(nul);
    }
    // Remove all trailing newlines (and a CR immediately before each).
    while result.ends_with('\n') {
        result.pop();
        if result.ends_with('\r') {
            result.pop();
        }
    }
    // Turn remaining newlines into single spaces (`\r\n` collapses; a lone `\r` is preserved).
    Ok(result.replace("\r\n", " ").replace('\n', " "))
}

/// `$(if COND,THEN[,ELSE])`: if COND (arg-text trimmed, then expanded) is non-empty, expand and
/// return THEN, else ELSE (or empty). Only the taken branch is expanded; THEN/ELSE keep whitespace.
fn func_if(raw_args: &str, open: char, vars: &Vars, ctx: &mut Expander) -> Result<String, String> {
    let parts = split_function_args(raw_args, 3, open);
    if parts.len() < 2 {
        return Err(format!(
            "insufficient number of arguments ({}) to function 'if'.",
            parts.len()
        ));
    }
    if !expand_inner(parts[0].trim(), vars, ctx)?.is_empty() {
        expand_inner(parts[1], vars, ctx)
    } else {
        expand_inner(parts.get(2).copied().unwrap_or(""), vars, ctx)
    }
}

/// `$(or A,B,...)`: return the first argument (arg-text trimmed, then expanded) that is non-empty,
/// short-circuiting. `$(and A,B,...)`: empty on the first empty argument, else the last one.
fn func_or(raw_args: &str, open: char, vars: &Vars, ctx: &mut Expander) -> Result<String, String> {
    for part in split_function_args(raw_args, usize::MAX, open) {
        let value = expand_inner(part.trim(), vars, ctx)?;
        if !value.is_empty() {
            return Ok(value);
        }
    }
    Ok(String::new())
}

fn func_and(raw_args: &str, open: char, vars: &Vars, ctx: &mut Expander) -> Result<String, String> {
    let mut last = String::new();
    for part in split_function_args(raw_args, usize::MAX, open) {
        last = expand_inner(part.trim(), vars, ctx)?;
        if last.is_empty() {
            return Ok(String::new());
        }
    }
    Ok(last)
}

/// `$(foreach VAR,LIST,TEXT)`: bind VAR to each word of LIST and expand TEXT per word, joining the
/// results with single spaces. The binding is local (a cloned scope), so it never leaks.
fn func_foreach(
    raw_args: &str,
    open: char,
    vars: &Vars,
    ctx: &mut Expander,
) -> Result<String, String> {
    let parts = split_function_args(raw_args, 3, open);
    if parts.len() < 3 {
        return Err(format!(
            "insufficient number of arguments ({}) to function 'foreach'.",
            parts.len()
        ));
    }
    let var = expand_inner(parts[0], vars, ctx)?;
    let var = var.trim();
    let list = expand_inner(parts[1], vars, ctx)?;
    let text = parts[2];

    let mut scope = vars.clone();
    let mut results = Vec::new();
    for word in list.split_whitespace() {
        scope.set(var, word, false)?;
        results.push(expand_inner(text, &scope, ctx)?);
    }
    Ok(results.join(" "))
}

/// `$(call NAME,P1,P2,...)`: expand the value of variable NAME (or invoke a built-in function of
/// that name) with `$(0)` bound to NAME and `$(1)`,`$(2)`,... to the parameters (expanded in the
/// caller's scope, whitespace preserved).
fn func_call(
    raw_args: &str,
    open: char,
    vars: &Vars,
    ctx: &mut Expander,
) -> Result<String, String> {
    let parts = split_function_args(raw_args, usize::MAX, open);
    let name = expand_inner(parts[0], vars, ctx)?;
    let name = name.trim();

    // Positional parameters, expanded once in the caller's scope.
    let mut params = Vec::with_capacity(parts.len().saturating_sub(1));
    for param in &parts[1..] {
        params.push(expand_inner(param, vars, ctx)?);
    }

    // A call to a built-in function (not shadowed by a variable) forwards the parameters as its
    // arguments — a text function directly, a control function via its comma-joined raw form.
    if !vars.is_set(name) {
        if function_arity(name).is_some() {
            return call_function(name, &params);
        }
        if is_control_function(name) {
            let joined = params.join(",");
            let result = call_control(name, &joined, open, vars, ctx)?;
            return Ok(result.unwrap_or_default());
        }
    }

    let body = vars.get(name).value.clone();
    let mut scope = vars.clone();
    // Start from a clean set of positional parameters so a nested call does not inherit the
    // enclosing call's higher-numbered parameters.
    scope.clear_call_params();
    scope.set("0", name, false)?;
    for (index, value) in params.into_iter().enumerate() {
        scope.set((index + 1).to_string(), value, false)?;
    }
    expand_inner(&body, &scope, ctx)
}

/// Look up a variable name for `$(value)`/`$(flavor)`: leading whitespace of the argument is
/// stripped but trailing is significant, so a name with trailing whitespace can never match.
fn lookup_name(raw_args: &str, vars: &Vars, ctx: &mut Expander) -> Result<String, String> {
    let expanded = expand_inner(raw_args, vars, ctx)?;
    let name = expanded.trim_start();
    // A trailing space makes the name unmatchable (real variable names have none).
    Ok(if name == name.trim_end() {
        name.to_string()
    } else {
        String::new()
    })
}

/// `$(value VAR)`: the raw, unexpanded value stored for VAR (no reference/function expansion).
fn func_value(raw_args: &str, vars: &Vars, ctx: &mut Expander) -> Result<String, String> {
    let name = lookup_name(raw_args, vars, ctx)?;
    Ok(vars.get(&name).value.clone())
}

/// `$(flavor VAR)`: `undefined`, `recursive`, or `simple`.
///
/// Note: omake flavors environment and built-in default variables as `simple`, whereas GNU make
/// reports them `recursive` — omake's variable store keeps only a recursive flag, not the origin.
fn func_flavor(raw_args: &str, vars: &Vars, ctx: &mut Expander) -> Result<String, String> {
    let name = lookup_name(raw_args, vars, ctx)?;
    let flavor = if !vars.is_set(&name) {
        "undefined"
    } else if vars.get(&name).recursive {
        "recursive"
    } else {
        "simple"
    };
    Ok(flavor.to_string())
}

/// `$(origin VAR)`: where VAR was defined — `undefined`, `default`, `environment`, `file`, or
/// `automatic`. (GNU's `command line`, `override`, and `environment override` origins need
/// features omake lacks; automatic variables are recognized by name.)
fn func_origin(raw_args: &str, vars: &Vars, ctx: &mut Expander) -> Result<String, String> {
    let name = lookup_name(raw_args, vars, ctx)?;
    let origin = if !vars.is_set(&name) {
        // An automatic variable is `undefined` until it is actually bound (i.e. inside a recipe),
        // matching GNU make.
        "undefined"
    } else if is_automatic_var(&name) {
        "automatic"
    } else {
        match vars.get(&name).origin {
            Origin::Default => "default",
            Origin::Environment => "environment",
            Origin::EnvironmentOverride => "environment override",
            Origin::File => "file",
            Origin::CommandLine => "command line",
            Origin::Override => "override",
        }
    };
    Ok(origin.to_string())
}

/// Whether `name` is an automatic variable (`$@`, `$<`, `$^`, ... and the `D`/`F` variants). When
/// one is bound (in a recipe), `$(origin)` reports it as `automatic` rather than by its stored
/// origin.
fn is_automatic_var(name: &str) -> bool {
    const CORE: [&str; 8] = ["@", "%", "<", "?", "^", "+", "|", "*"];
    CORE.contains(&name)
        || (name.len() == 2 && name.ends_with(['D', 'F']) && CORE.contains(&&name[..1]))
}

/// `$(eval TEXT)`: expand TEXT, then hand the result to the makefile parser (via the eval sink;
/// the caller drains it after expansion finishes). Expands to the empty string.
///
/// Note: the effect is applied after the enclosing expansion, so a variable defined by `$(eval)` is
/// visible on later lines but not within the same expression (GNU make applies it immediately). One
/// consequence: a single-dollar accumulator inside a `$(foreach ...,$(eval ...))` loop (each
/// iteration reading a variable a prior iteration wrote via `$(VAR)`) does not accumulate; the
/// portable double-dollar form (`$$(VAR)`) does. Only makefile-parse contexts that go through
/// `Makefile::expand_str` (assignments, rule lines, standalone lines) drain the sink; a `$(eval)`
/// in a recipe or a condition/`include` operand is a no-op.
fn func_eval(raw_args: &str, vars: &Vars, ctx: &mut Expander) -> Result<String, String> {
    let text = expand_inner(raw_args, vars, ctx)?;
    ctx.evals.push(text);
    Ok(String::new())
}

/// Which stream a diagnostic function writes to.
enum Stream {
    Stdout,
    Stderr,
}

/// `$(info TEXT)` (stdout) / `$(warning TEXT)` (stderr): print the expanded message and return the
/// empty string. Leading whitespace of TEXT is stripped; trailing whitespace is preserved.
///
/// Note: GNU prefixes `$(warning)` with `file:line:`; omake's expander has no source context, so
/// the message is printed without that prefix.
fn func_diagnostic(
    raw_args: &str,
    vars: &Vars,
    ctx: &mut Expander,
    stream: Stream,
) -> Result<String, String> {
    let message = expand_inner(raw_args.trim_start(), vars, ctx)?;
    match stream {
        Stream::Stdout => println!("{message}"),
        Stream::Stderr => eprintln!("{message}"),
    }
    Ok(String::new())
}

/// `$(realpath NAMES)`: the canonical absolute path of each name (symlinks resolved). A name that
/// does not exist contributes nothing, so the word count may be less than the input count.
fn func_realpath(names: &str) -> String {
    names
        .split_whitespace()
        .filter_map(|name| {
            std::fs::canonicalize(name)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `$(abspath NAMES)`: the absolute path of each name, resolved purely lexically (`.`/`..`
/// collapsed, no symlink resolution, no filesystem access). Always one output word per input.
fn func_abspath(names: &str) -> String {
    names
        .split_whitespace()
        .map(abspath_one)
        .collect::<Vec<_>>()
        .join(" ")
}

fn abspath_one(name: &str) -> String {
    // Make absolute against the current directory, then normalize components textually.
    let combined = if name.starts_with('/') {
        name.to_string()
    } else {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{cwd}/{name}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for comp in combined.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    format!("/{}", parts.join("/"))
}

/// `$(file OP PATH[,TEXT])`: write (`>`), append (`>>`), or read (`<`) a file, performed during
/// expansion. Write/append return the empty string; read returns the contents with one trailing
/// newline removed. A missing or invalid operator is an error.
fn func_file(arg: &str) -> Result<String, String> {
    let arg = arg.trim_start();
    let (append, rest) = if let Some(rest) = arg.strip_prefix(">>") {
        (Some(true), rest)
    } else if let Some(rest) = arg.strip_prefix('>') {
        (Some(false), rest)
    } else if let Some(rest) = arg.strip_prefix('<') {
        // Read: the path is the first whitespace-delimited token; extra tokens are ignored. A `>`
        // in the operator position (`<>`) is an invalid combined operator; a missing path errors.
        let rest = rest.trim_start();
        if rest.starts_with('>') {
            return Err("file: invalid file operation".to_string());
        }
        let path = rest.split_whitespace().next().unwrap_or("");
        if path.is_empty() {
            return Err("file: missing filename".to_string());
        }
        let content = std::fs::read_to_string(path).unwrap_or_default();
        return Ok(content.strip_suffix('\n').unwrap_or(&content).to_string());
    } else {
        return Err("file: invalid file operation".to_string());
    };

    // Write/append: the path is up to the first comma (whitespace-trimmed, first token); the text
    // after the comma is literal. A trailing newline is added unless the text already ends in one.
    let (path_part, data) = match rest.split_once(',') {
        // A trailing newline is added unless the text already ends in one (so empty text writes a
        // lone newline).
        Some((path, text)) if text.ends_with('\n') => (path, text.to_string()),
        Some((path, text)) => (path, format!("{text}\n")),
        // No comma at all: write nothing (a 0-byte file, still truncating for `>`).
        None => (rest, String::new()),
    };
    let path = path_part.split_whitespace().next().unwrap_or("");

    let file = if append == Some(true) {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    } else {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
    };
    use std::io::Write;
    file.and_then(|mut f| f.write_all(data.as_bytes()))
        .map_err(|e| format!("file: {e}"))?;
    Ok(String::new())
}

/// `$(wildcard PATTERN...)`: expand each whitespace-separated glob pattern against the filesystem,
/// concatenating the (sorted, existing) matches. A pattern that matches nothing contributes
/// nothing.
fn wildcard(patterns: &str) -> String {
    patterns
        .split_whitespace()
        .flat_map(crate::glob::glob)
        .collect::<Vec<_>>()
        .join(" ")
}

/// What follows the `:` in a `${VAR:...}` reference: nothing, a GNU substitution reference, or a
/// chain of BSD variable modifiers.
enum VarAction {
    None,
    Gnu(String, String),
    Bsd(Vec<Modifier>),
}

/// A BSD variable modifier (`:M`, `:S`, `:Q`, `:tl`, ...).
enum Modifier {
    /// `:Mpattern` — keep words matching the glob pattern.
    Match(String),
    /// `:Npattern` — keep words not matching the glob pattern.
    NoMatch(String),
    /// `:S/from/to/[1g]` — literal substring substitution per word (`^`/`$` anchor to word bounds).
    Subst {
        from: String,
        to: String,
        global: bool,
        anchor_start: bool,
        anchor_end: bool,
    },
    /// `:Q` — backslash-escape shell-special characters in the value.
    Quote,
    /// `:tl` / `:tu` — lower/upper case.
    Lower,
    Upper,
    /// `:H` / `:T` — directory / file part of each word.
    Head,
    Tail,
    /// `:E` / `:R` — extension / root (basename without extension) of each word.
    Ext,
    Root,
    /// `:Uvalue` — use `value` if the variable is undefined.
    Default(String),
}

/// Determine the variable name and the action (GNU substitution reference or BSD modifiers) from a
/// `${...}` reference's content. A `=` in the part after the first `:` marks a GNU reference; a valid
/// BSD modifier chain (with no `=`) is BSD; anything else leaves the `:` as part of the name.
fn parse_var_action(content: &str) -> (String, VarAction) {
    match content.split_once(':') {
        Some((name, m)) if m.contains('=') => {
            let (from, to) = m.split_once('=').unwrap();
            (
                name.trim().to_string(),
                VarAction::Gnu(from.to_string(), to.to_string()),
            )
        }
        Some((name, m)) => match parse_bsd_modifiers(m) {
            Some(mods) => (name.trim().to_string(), VarAction::Bsd(mods)),
            None => (content.to_string(), VarAction::None),
        },
        None => (content.to_string(), VarAction::None),
    }
}

/// Parse a chain of BSD modifiers (`M...:S/.../.../...`). Returns `None` if any part is not a
/// recognized modifier, so the caller can fall back to treating the `:` as part of the name.
fn parse_bsd_modifiers(mut rest: &str) -> Option<Vec<Modifier>> {
    let mut mods = Vec::new();
    while !rest.is_empty() {
        let (modifier, next) = take_modifier(rest)?;
        mods.push(modifier);
        rest = next;
    }
    (!mods.is_empty()).then_some(mods)
}

/// Parse one BSD modifier from the start of `s`; return it and the remainder after the separating
/// `:` (empty at the end).
fn take_modifier(s: &str) -> Option<(Modifier, &str)> {
    // A fixed-token modifier must be followed by `:` or end of string.
    let simple = |len: usize, m: Modifier| -> Option<(Modifier, &str)> {
        match s[len..].strip_prefix(':') {
            Some(after) => Some((m, after)),
            None if s.len() == len => Some((m, "")),
            None => None,
        }
    };
    // A one-argument modifier (`M`/`N`/`U`) takes everything up to the next `:`.
    let arg = |m: fn(String) -> Modifier| -> Option<(Modifier, &str)> {
        match s[1..].find(':') {
            Some(i) => Some((m(s[1..1 + i].to_string()), &s[2 + i..])),
            None => Some((m(s[1..].to_string()), "")),
        }
    };
    match s.as_bytes().first()? {
        b'Q' => simple(1, Modifier::Quote),
        b'H' => simple(1, Modifier::Head),
        b'T' => simple(1, Modifier::Tail),
        b'E' => simple(1, Modifier::Ext),
        b'R' => simple(1, Modifier::Root),
        b't' if s.as_bytes().get(1) == Some(&b'l') => simple(2, Modifier::Lower),
        b't' if s.as_bytes().get(1) == Some(&b'u') => simple(2, Modifier::Upper),
        b'M' => arg(Modifier::Match),
        b'N' => arg(Modifier::NoMatch),
        b'U' => arg(Modifier::Default),
        b'S' => take_subst(s),
        _ => None,
    }
}

/// Parse a `:S<sep>from<sep>to<sep>[1g]` substitution modifier.
fn take_subst(s: &str) -> Option<(Modifier, &str)> {
    let mut chars = s.char_indices();
    chars.next()?; // 'S'
    let (sep_pos, sep) = chars.next()?; // separator character
    let after = &s[sep_pos + sep.len_utf8()..];
    let (from, after) = split_once_char(after, sep)?;
    let (to, after) = split_once_char(after, sep)?;
    // Flags run to the next `:` or end.
    let (flags, rest) = match after.find(':') {
        Some(i) => (&after[..i], &after[i + 1..]),
        None => (after, ""),
    };
    let anchor_start = from.starts_with('^');
    let from = from.strip_prefix('^').unwrap_or(from);
    let anchor_end = from.ends_with('$');
    let from = from.strip_suffix('$').unwrap_or(from);
    Some((
        Modifier::Subst {
            from: from.to_string(),
            to: to.to_string(),
            global: flags.contains('g'),
            anchor_start,
            anchor_end,
        },
        rest,
    ))
}

/// Split `s` at the first `sep`, returning the part before and after (or `None` if `sep` is absent).
fn split_once_char(s: &str, sep: char) -> Option<(&str, &str)> {
    let i = s.find(sep)?;
    Some((&s[..i], &s[i + sep.len_utf8()..]))
}

/// Apply a chain of BSD modifiers to `value` (`is_set` is whether the variable is defined, for `:U`).
fn apply_bsd_modifiers(value: &str, mods: &[Modifier], is_set: bool) -> String {
    let mut val = value.to_string();
    let mut set = is_set;
    // Empty results drop out of the word list, matching bmake.
    let map_words = |val: &str, f: &dyn Fn(&str) -> String| {
        val.split_whitespace()
            .map(f)
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    for m in mods {
        match m {
            Modifier::Default(def) => {
                if !set {
                    val = def.clone();
                    set = true;
                }
            }
            Modifier::Quote => val = bsd_quote(&val),
            Modifier::Lower => val = val.to_lowercase(),
            Modifier::Upper => val = val.to_uppercase(),
            Modifier::Match(pat) => {
                val = val
                    .split_whitespace()
                    .filter(|w| crate::glob::fnmatch(pat, w))
                    .collect::<Vec<_>>()
                    .join(" ");
            }
            Modifier::NoMatch(pat) => {
                val = val
                    .split_whitespace()
                    .filter(|w| !crate::glob::fnmatch(pat, w))
                    .collect::<Vec<_>>()
                    .join(" ");
            }
            Modifier::Subst {
                from,
                to,
                global,
                anchor_start,
                anchor_end,
            } => {
                val = map_words(&val, &|w| {
                    bsd_subst(w, from, to, *global, *anchor_start, *anchor_end)
                });
            }
            Modifier::Head => val = map_words(&val, &|w| path_head(w).to_string()),
            Modifier::Tail => val = map_words(&val, &|w| path_tail(w).to_string()),
            Modifier::Ext => val = map_words(&val, &|w| path_ext(w).to_string()),
            Modifier::Root => val = map_words(&val, &|w| path_root(w).to_string()),
        }
    }
    val
}

/// Literal substring substitution for a `:S` modifier (`^`/`$` anchor to the word's bounds).
fn bsd_subst(word: &str, from: &str, to: &str, global: bool, start: bool, end: bool) -> String {
    match (start, end) {
        (true, true) => {
            if word == from {
                to.to_string()
            } else {
                word.to_string()
            }
        }
        (true, false) => match word.strip_prefix(from) {
            Some(rest) => format!("{to}{rest}"),
            None => word.to_string(),
        },
        (false, true) => match word.strip_suffix(from) {
            Some(pre) => format!("{pre}{to}"),
            None => word.to_string(),
        },
        (false, false) if global => word.replace(from, to),
        (false, false) => word.replacen(from, to, 1),
    }
}

/// Backslash-escape shell-special characters for a `:Q` modifier.
fn bsd_quote(value: &str) -> String {
    const SPECIAL: &[char] = &[
        ' ', '\t', '\n', '"', '\'', '`', '\\', '$', ';', '&', '|', '<', '>', '(', ')', '{', '}',
        '[', ']', '*', '?', '#', '~', '^', '!',
    ];
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if SPECIAL.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Directory part of a word (`:H`) — everything before the last `/`; `.` when there is no `/`.
fn path_head(word: &str) -> &str {
    match word.rfind('/') {
        Some(i) => &word[..i],
        None => ".",
    }
}

/// File part of a word (`:T`); the whole word when there is no `/`.
fn path_tail(word: &str) -> &str {
    match word.rfind('/') {
        Some(i) => &word[i + 1..],
        None => word,
    }
}

/// Extension of a word without the dot (`:E`); empty when the basename has no `.`.
fn path_ext(word: &str) -> &str {
    let base = path_tail(word);
    match base.rfind('.') {
        Some(i) => &base[i + 1..],
        None => "",
    }
}

/// Root of a word — everything but the extension (`:R`).
fn path_root(word: &str) -> &str {
    let dot = path_tail(word).rfind('.');
    match dot {
        Some(i) => {
            let cut = word.len() - (path_tail(word).len() - i);
            &word[..cut]
        }
        None => word,
    }
}

/// Apply a substitution reference (`$(VAR:FROM=TO)`) to each whitespace-separated word of `value`.
fn substitute_ref(value: &str, from: &str, to: &str) -> String {
    value
        .split_whitespace()
        .map(|word| substitute_word(word, from, to))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A make `%`-pattern after backslash escapes are resolved: a wildcard pattern (split around the
/// single unescaped `%`) or a literal string (every `%` was escaped, or there were none).
enum Pattern {
    Wildcard { prefix: String, suffix: String },
    Literal(String),
}

/// Resolve `\%` escapes in a make pattern. A backslash quotes a following `%` (so `\%` is a literal
/// percent and `\\%` is a literal backslash before the wildcard); the first *unescaped* `%` is the
/// wildcard. Backslashes not before a `%` are kept verbatim, matching GNU make.
fn parse_pattern(pat: &str) -> Pattern {
    let mut text = String::with_capacity(pat.len());
    let mut wildcard = None;
    let mut backslashes = 0usize;
    for c in pat.chars() {
        match c {
            '\\' => backslashes += 1,
            '%' => {
                // A run of N backslashes becomes N/2 literal backslashes; an odd one quotes the `%`.
                for _ in 0..backslashes / 2 {
                    text.push('\\');
                }
                if backslashes.is_multiple_of(2) && wildcard.is_none() {
                    wildcard = Some(text.len());
                }
                backslashes = 0;
                text.push('%');
            }
            _ => {
                for _ in 0..backslashes {
                    text.push('\\');
                }
                backslashes = 0;
                text.push(c);
            }
        }
    }
    for _ in 0..backslashes {
        text.push('\\');
    }
    match wildcard {
        Some(pos) => Pattern::Wildcard {
            prefix: text[..pos].to_string(),
            suffix: text[pos + 1..].to_string(),
        },
        None => Pattern::Literal(text),
    }
}

/// Substitute one word for a substitution reference. A wildcard `from` has its stem substituted into
/// `to`'s `%`; a literal `from` is a suffix replaced by `to`. A word that does not match is returned
/// unchanged.
fn substitute_word(word: &str, from: &str, to: &str) -> String {
    match parse_pattern(from) {
        Pattern::Wildcard { prefix, suffix } => match stem_of(&prefix, &suffix, word) {
            Some(stem) => subst_replacement(to, stem),
            None => word.to_string(),
        },
        Pattern::Literal(text) => match word.strip_suffix(&text) {
            Some(prefix) => format!("{prefix}{to}"),
            None => word.to_string(),
        },
    }
}

/// If `word` matches the wildcard pattern `prefix`%`suffix`, return the matched stem.
fn stem_of<'a>(prefix: &str, suffix: &str, word: &'a str) -> Option<&'a str> {
    if word.len() >= prefix.len() + suffix.len()
        && word.starts_with(prefix)
        && word.ends_with(suffix)
    {
        Some(&word[prefix.len()..word.len() - suffix.len()])
    } else {
        None
    }
}

/// Build a replacement: substitute `stem` into the replacement's wildcard `%` (escapes resolved), or
/// return the replacement literally when it has no wildcard.
fn subst_replacement(repl: &str, stem: &str) -> String {
    match parse_pattern(repl) {
        Pattern::Wildcard { prefix, suffix } => format!("{prefix}{stem}{suffix}"),
        Pattern::Literal(text) => text,
    }
}

/// Whether `word` matches `pattern`: a wildcard pattern (any stem) or a literal exact match.
fn word_matches(pattern: &str, word: &str) -> bool {
    match parse_pattern(pattern) {
        Pattern::Wildcard { prefix, suffix } => stem_of(&prefix, &suffix, word).is_some(),
        Pattern::Literal(text) => text == word,
    }
}

/// Split `s` into at most `max` comma-separated arguments, ignoring commas nested inside the
/// delimiter pair that opened this call (`(...)` for `open == '('`, `{...}` for `'{'`). GNU make
/// balances only the opener's own delimiter, so a stray brace inside a `$(...)` call is literal.
/// The final argument keeps any remaining commas.
fn split_function_args(s: &str, max: usize, open: char) -> Vec<&str> {
    let close = if open == '(' { ')' } else { '}' };
    let mut args = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if args.len() + 1 == max {
            break;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
        } else if c == ',' && depth == 0 {
            args.push(&s[start..i]);
            start = i + 1;
        }
    }
    args.push(&s[start..]);
    args
}

/// Fetch argument `i` (0-based) from a split argument list, or `""` if absent.
fn arg(args: &[String], i: usize) -> &str {
    args.get(i).map(String::as_str).unwrap_or("")
}

/// `$(subst FROM,TO,TEXT)`: replace every occurrence of the literal `FROM` with `TO`.
fn func_subst(args: &[String]) -> String {
    let (from, to, text) = (arg(args, 0), arg(args, 1), arg(args, 2));
    if from.is_empty() {
        // GNU appends `to` once when `from` is empty (rather than inserting between every char).
        format!("{text}{to}")
    } else {
        text.replace(from, to)
    }
}

/// `$(patsubst PATTERN,REPLACEMENT,TEXT)`: per word, replace a match of `PATTERN` (with `%`, else
/// an exact word) by `REPLACEMENT`. Unlike the other word functions, patsubst preserves TEXT's
/// original whitespace layout (leading, trailing, and the runs between words).
fn func_patsubst(args: &[String]) -> String {
    let (pattern, replacement, text) = (arg(args, 0), arg(args, 1), arg(args, 2));
    let pattern = parse_pattern(pattern);
    map_words_keep_spaces(text, |word| match &pattern {
        Pattern::Wildcard { prefix, suffix } => match stem_of(prefix, suffix, word) {
            Some(stem) => subst_replacement(replacement, stem),
            None => word.to_string(),
        },
        Pattern::Literal(lit) if word == lit => replacement.to_string(),
        Pattern::Literal(_) => word.to_string(),
    })
}

/// Apply `f` to each whitespace-separated word of `text`, preserving every whitespace run (leading,
/// trailing, and between words) verbatim.
fn map_words_keep_spaces(text: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let ws_end = rest
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(rest.len());
        out.push_str(&rest[..ws_end]);
        rest = &rest[ws_end..];
        if rest.is_empty() {
            break;
        }
        let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        out.push_str(&f(&rest[..word_end]));
        rest = &rest[word_end..];
    }
    out
}

/// `$(strip TEXT)`: collapse runs of whitespace to a single space and trim the ends.
fn func_strip(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `$(sort LIST)`: sort words and remove duplicates.
fn func_sort(list: &str) -> String {
    let mut words: Vec<&str> = list.split_whitespace().collect();
    words.sort_unstable();
    words.dedup();
    words.join(" ")
}

/// `$(filter PATTERNS,TEXT)` / `$(filter-out PATTERNS,TEXT)`: keep (or drop) words matching any
/// pattern.
fn func_filter(args: &[String], out: bool) -> String {
    let (patterns, text) = (arg(args, 0), arg(args, 1));
    let patterns: Vec<&str> = patterns.split_whitespace().collect();
    text.split_whitespace()
        .filter(|word| patterns.iter().any(|p| word_matches(p, word)) != out)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `$(words TEXT)`: the number of words.
fn func_words(text: &str) -> String {
    text.split_whitespace().count().to_string()
}

/// `$(word N,TEXT)`: the Nth word (1-based). Errors on a non-numeric or zero index, like GNU make.
fn func_word(args: &[String]) -> Result<String, String> {
    let n = word_index("word", arg(args, 0))?;
    Ok(arg(args, 1)
        .split_whitespace()
        .nth(n - 1)
        .unwrap_or("")
        .to_string())
}

/// `$(wordlist S,E,TEXT)`: words S through E inclusive (1-based). `E` before `S` yields nothing.
fn func_wordlist(args: &[String]) -> Result<String, String> {
    let start = word_index("wordlist", arg(args, 0))?;
    let end = word_index("wordlist", arg(args, 1))?;
    if end < start {
        return Ok(String::new());
    }
    Ok(arg(args, 2)
        .split_whitespace()
        .skip(start - 1)
        .take(end - start + 1)
        .collect::<Vec<_>>()
        .join(" "))
}

/// Parse a 1-based word index for `word`/`wordlist`, erroring on a non-numeric or zero value.
fn word_index(func: &str, value: &str) -> Result<usize, String> {
    let value = value.trim();
    let n: usize = value
        .parse()
        .map_err(|_| format!("invalid first argument to '{func}' function: '{value}'."))?;
    if n == 0 {
        return Err(format!(
            "first argument to '{func}' function must be greater than 0."
        ));
    }
    Ok(n)
}

/// `$(firstword TEXT)` / `$(lastword TEXT)`.
fn func_firstword(text: &str) -> String {
    text.split_whitespace().next().unwrap_or("").to_string()
}
fn func_lastword(text: &str) -> String {
    text.split_whitespace()
        .next_back()
        .unwrap_or("")
        .to_string()
}

/// `$(dir NAMES)`: the directory part of each word (up to and including the last `/`), or `./`.
fn func_dir(args: &str) -> String {
    map_words(args, |word| match word.rfind('/') {
        Some(i) => word[..=i].to_string(),
        None => "./".to_string(),
    })
}

/// `$(notdir NAMES)`: the part of each word after the last `/`.
fn func_notdir(args: &str) -> String {
    map_words(args, |word| match word.rfind('/') {
        Some(i) => word[i + 1..].to_string(),
        None => word.to_string(),
    })
}

/// `$(basename NAMES)`: each word without its suffix (the last `.` in the final path component).
fn func_basename(args: &str) -> String {
    map_words(args, |word| {
        let base = word.rfind('/').map_or(0, |i| i + 1);
        match word[base..].rfind('.') {
            Some(dot) => word[..base + dot].to_string(),
            None => word.to_string(),
        }
    })
}

/// `$(suffix NAMES)`: the suffix of each word (from the last `.`); words with no suffix contribute
/// nothing.
fn func_suffix(args: &str) -> String {
    args.split_whitespace()
        .filter_map(|word| {
            let base = word.rfind('/').map_or(0, |i| i + 1);
            word[base..].rfind('.').map(|dot| &word[base + dot..])
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `$(addprefix PREFIX,NAMES)` / `$(addsuffix SUFFIX,NAMES)`.
fn func_addprefix(args: &[String]) -> String {
    let prefix = arg(args, 0);
    map_words(arg(args, 1), |word| format!("{prefix}{word}"))
}
fn func_addsuffix(args: &[String]) -> String {
    let suffix = arg(args, 0);
    map_words(arg(args, 1), |word| format!("{word}{suffix}"))
}

/// `$(join LIST1,LIST2)`: concatenate the two lists word by word; extra words pass through.
fn func_join(args: &[String]) -> String {
    let w1: Vec<&str> = arg(args, 0).split_whitespace().collect();
    let w2: Vec<&str> = arg(args, 1).split_whitespace().collect();
    (0..w1.len().max(w2.len()))
        .map(|i| format!("{}{}", w1.get(i).unwrap_or(&""), w2.get(i).unwrap_or(&"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Apply `f` to each whitespace-separated word of `text`, joining the results with a single space.
fn map_words(text: &str, f: impl Fn(&str) -> String) -> String {
    text.split_whitespace().map(f).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_single_letter_expansions() {
        let vars = Vars::new([("A", "VALUE A"), ("B", "VALUE B")]);
        assert_eq!(expand("$A", &vars).unwrap(), "VALUE A");
        assert_eq!(
            expand("$A with some text.", &vars).unwrap(),
            "VALUE A with some text.",
        );
        assert_eq!(
            expand("Some leading text and $A.", &vars).unwrap(),
            "Some leading text and VALUE A.",
        );
        assert_eq!(
            expand("Both vars: $A and $B.", &vars).unwrap(),
            "Both vars: VALUE A and VALUE B.",
        );
    }

    #[test]
    fn test_basic_long_expansions() {
        let vars = Vars::new([("TESTA", "VALUE A"), ("TESTB", "VALUE B")]);
        assert_eq!(expand("$(TESTA)", &vars).unwrap(), "VALUE A");
        assert_eq!(
            expand("${TESTA} and $(TESTB)", &vars).unwrap(),
            "VALUE A and VALUE B",
        );
        assert_eq!(
            expand("Leading text and $(TESTA) and $(TESTB).", &vars).unwrap(),
            "Leading text and VALUE A and VALUE B.",
        );
    }

    #[test]
    fn test_basic_nested_expansions() {
        let vars = Vars::new([("A", "B"), ("B", "VALUE1"), ("CD", "VALUE2"), ("E", "D")]);
        assert_eq!(
            expand("This is $($(A))!", &vars).unwrap(),
            "This is VALUE1!",
        );

        // Test nested with both parentheses and braces.
        assert_eq!(
            expand("This is $(${A})!", &vars).unwrap(),
            "This is VALUE1!",
        );
        assert_eq!(
            expand("This is ${$(A)}!", &vars).unwrap(),
            "This is VALUE1!",
        );
        assert_eq!(
            expand("This is ${${A}}!", &vars).unwrap(),
            "This is VALUE1!",
        );

        // Test nested with nested string literal.
        assert_eq!(
            expand("This is ${C$(E)}!", &vars).unwrap(),
            "This is VALUE2!",
        );
    }

    #[test]
    fn test_escape_dollar_sign() {
        let vars = Vars::new([("A", "B")]);
        assert_eq!(expand("This is $$A!", &vars).unwrap(), "This is $A!");
        assert_eq!(expand("This is $${A}!", &vars).unwrap(), "This is ${A}!");
        assert_eq!(expand("This is $$${A}!", &vars).unwrap(), "This is $B!");
    }

    #[test]
    fn test_not_recursive() {
        let vars = Vars::new([("A", "B"), ("C", "${A}")]);
        assert_eq!(expand("Test ${C}", &vars).unwrap(), "Test ${A}");
    }

    #[test]
    fn test_recursive() {
        let mut vars = Vars::new([("A", "B")]);
        vars.set("C", "${A}", true).unwrap();
        assert_eq!(expand("Test ${C}", &vars).unwrap(), "Test B");
    }

    #[test]
    fn test_double_recursive() {
        let mut vars = Vars::new([("A", "B")]);
        for (k, v) in [("C", "${A}"), ("D", "$(C)")] {
            vars.set(k, v, true).unwrap();
        }
        assert_eq!(expand("Test ${D}", &vars).unwrap(), "Test B");
    }

    #[test]
    fn test_intermediate_not_recursive() {
        let mut vars = Vars::new([("C", "${A}")]);
        for (k, v) in [("A", "B"), ("D", "$(C)")] {
            vars.set(k, v, true).unwrap();
        }
        assert_eq!(expand("Test ${D}", &vars).unwrap(), "Test ${A}");
    }

    #[test]
    fn test_recursion_on_simple_value_works() {
        let mut vars = Vars::new([]);
        for (k, v) in [("A", "B"), ("C", "${A}"), ("D", "$(C)")] {
            vars.set(k, v, true).unwrap();
        }
        assert_eq!(expand("Test ${D}", &vars).unwrap(), "Test B");
    }

    #[test]
    fn test_nested_variable_without_closing_delimiter() {
        let vars = Vars::new([("TEST", "Value")]);
        assert!(expand("${TEST", &vars).is_err());
    }

    #[test]
    fn test_dollar_space_empty_replacement() {
        let vars = Vars::new([]);
        assert_eq!(expand("one$ word", &vars).unwrap(), "oneword");
    }

    #[test]
    fn test_trailing_dollar_is_literal() {
        let vars = Vars::new([("A", "VALUE A")]);
        assert_eq!(expand("foo$", &vars).unwrap(), "foo$");
        assert_eq!(expand("$", &vars).unwrap(), "$");
    }

    #[test]
    fn test_unmatched_closing_delimiter_after_dollar() {
        // A `$` before an orphan `)`/`}` (no open expression) must not consume the next char.
        let vars = Vars::new([("A", "VALUE A")]);
        assert_eq!(expand("$)A", &vars).unwrap(), ")A");
        assert_eq!(expand("$}A", &vars).unwrap(), "}A");
    }

    #[test]
    fn test_substitution_reference() {
        let vars = Vars::new([("SRCS", "a.c b.c dir/c.c"), ("MIX", "a.c b.h")]);
        // Suffix form.
        assert_eq!(expand("$(SRCS:.c=.o)", &vars).unwrap(), "a.o b.o dir/c.o");
        assert_eq!(expand("${SRCS:.c=.o}", &vars).unwrap(), "a.o b.o dir/c.o");
        // Empty replacement strips the suffix.
        assert_eq!(expand("$(SRCS:.c=)", &vars).unwrap(), "a b dir/c");
        // Non-matching words are unchanged.
        assert_eq!(expand("$(MIX:.c=.o)", &vars).unwrap(), "a.o b.h");
        // Pattern form.
        assert_eq!(expand("$(SRCS:%.c=%.o)", &vars).unwrap(), "a.o b.o dir/c.o");
        assert_eq!(
            expand("$(SRCS:%.c=obj/%.o)", &vars).unwrap(),
            "obj/a.o obj/b.o obj/dir/c.o"
        );
    }

    #[test]
    fn test_recursive_self_reference_errors() {
        // A recursive variable that references itself must error, not overflow the stack.
        let mut vars = Vars::new([]);
        vars.set("A", "$(A)", true).unwrap();
        assert!(expand("$(A)", &vars).is_err());

        // Mutual recursion is also caught.
        let mut vars = Vars::new([]);
        vars.set("B", "$(C)", true).unwrap();
        vars.set("C", "$(B)", true).unwrap();
        assert!(expand("$(B)", &vars).is_err());
    }

    #[test]
    fn test_text_functions() {
        let vars = Vars::new([("SRCS", "a.c b.c c.h")]);
        let e = |s: &str| expand(s, &vars).unwrap();

        assert_eq!(e("$(subst ee,EE,feet street)"), "fEEt strEEt");
        assert_eq!(e("$(subst ,X,abc)"), "abcX");
        assert_eq!(e("$(patsubst %.c,%.o,a.c b.c c.h)"), "a.o b.o c.h");
        assert_eq!(e("$(patsubst foo,bar,foo baz)"), "bar baz");
        assert_eq!(e("$(strip   a   b  )"), "a b");
        assert_eq!(e("$(sort foo bar foo baz)"), "bar baz foo");
        assert_eq!(e("$(filter %.c,$(SRCS))"), "a.c b.c");
        assert_eq!(e("$(filter-out %.c,$(SRCS))"), "c.h");
        assert_eq!(e("$(words $(SRCS))"), "3");
        assert_eq!(e("$(word 2,a b c)"), "b");
        assert_eq!(e("$(word 9,a b c)"), "");
        assert_eq!(e("$(wordlist 2,3,a b c d)"), "b c");
        assert_eq!(e("$(firstword $(SRCS))"), "a.c");
        assert_eq!(e("$(lastword $(SRCS))"), "c.h");
        assert_eq!(e("$(dir src/x.c y.c)"), "src/ ./");
        assert_eq!(e("$(notdir src/x.c y.c)"), "x.c y.c");
        assert_eq!(e("$(basename src/x.tar.gz y)"), "src/x.tar y");
        assert_eq!(e("$(suffix src/x.c y)"), ".c");
        assert_eq!(e("$(addprefix -I,inc lib)"), "-Iinc -Ilib");
        assert_eq!(e("$(addsuffix .o,a b)"), "a.o b.o");
        assert_eq!(e("$(join a b c,1 2)"), "a1 b2 c");
    }

    #[test]
    fn test_nested_functions() {
        let vars = Vars::new([("SRCS", "src/a.c src/b.c inc/c.h")]);
        // Functions compose: filter the .c sources, then map them to .o via patsubst.
        assert_eq!(
            expand("$(patsubst %.c,%.o,$(filter %.c,$(SRCS)))", &vars).unwrap(),
            "src/a.o src/b.o"
        );
        // An unknown "function" name falls back to a (here empty) variable reference.
        assert_eq!(expand("$(notafunc a b)", &vars).unwrap(), "");
    }

    #[test]
    fn test_function_argument_parsing() {
        // A comma produced by expansion is NOT an argument separator: arguments are split from the
        // raw text before expansion (the classic `$(comma)` idiom).
        let vars = Vars::new([("comma", ",")]);
        assert_eq!(expand("$(subst $(comma),X,a,b)", &vars).unwrap(), "aXb");

        // Literal parentheses protect their commas and do not end the expression early.
        let vars = Vars::new([]);
        assert_eq!(expand("$(subst (a,b),Z,x(a,b)y)", &vars).unwrap(), "xZy");

        // Trailing whitespace in the last argument is significant.
        assert_eq!(expand("$(subst a,b,a )", &vars).unwrap(), "b ");

        // `word`/`wordlist` abort on a non-numeric or zero index (like GNU make).
        assert!(expand("$(word 0,a b)", &vars).is_err());
        assert!(expand("$(word x,a b)", &vars).is_err());
        assert!(expand("$(wordlist 0,2,a b)", &vars).is_err());
        // A valid but out-of-range index is empty, not an error.
        assert_eq!(expand("$(word 9,a b)", &vars).unwrap(), "");
    }

    #[test]
    fn test_control_functions() {
        let vars = Vars::new([("L", "a b c"), ("E", "")]);
        let e = |s: &str| expand(s, &vars).unwrap();

        // if: taken branch only; THEN/ELSE keep whitespace; COND arg-text trimmed then expanded.
        assert_eq!(e("$(if 1,yes,no)"), "yes");
        assert_eq!(e("$(if ,yes,no)"), "no");
        assert_eq!(e("$(if 1, yes , no )"), " yes ");
        assert_eq!(e("$(if $(E),yes,no)"), "no");
        assert!(expand("$(if 1)", &vars).is_err()); // too few arguments

        // or / and: short-circuit, return first-non-empty / last.
        assert_eq!(e("$(or ,,c,d)"), "c");
        assert_eq!(e("$(or ,,)"), "");
        assert_eq!(e("$(and a,b,c)"), "c");
        assert_eq!(e("$(and a,,c)"), "");

        // foreach: per-word binding, join with single spaces, no leak.
        assert_eq!(e("$(foreach x,$(L),<$(x)>)"), "<a> <b> <c>");
        assert_eq!(e("$(foreach x,,<$(x)>)"), "");
        assert_eq!(
            e("$(foreach a,1 2,$(foreach b,x y,$(a)$(b)))"),
            "1x 1y 2x 2y"
        );
        assert!(expand("$(foreach x,a b)", &vars).is_err());

        // call: positional params, $(0) = name, recursion.
        let vars2 = Vars::new([
            ("reverse", "$(2) $(1)"),
            ("named", "n=$(0) p=$(1)"),
            (
                "map",
                "$(if $(1),$(firstword $(1)) $(call map,$(wordlist 2,999,$(1))))",
            ),
        ]);
        assert_eq!(expand("$(call reverse,a,b)", &vars2).unwrap(), "b a");
        assert_eq!(expand("$(call named,X)", &vars2).unwrap(), "n=named p=X");
        assert_eq!(expand("$(call map,a b c)", &vars2).unwrap(), "a b c ");

        // value: raw, unexpanded text; flavor: recursive / simple / undefined.
        let mut vars3 = Vars::new([]);
        vars3.set("REC", "x $(L) y", true).unwrap();
        vars3.set("SIMPLE", "z", false).unwrap();
        assert_eq!(expand("$(value REC)", &vars3).unwrap(), "x $(L) y");
        assert_eq!(expand("$(flavor REC)", &vars3).unwrap(), "recursive");
        assert_eq!(expand("$(flavor SIMPLE)", &vars3).unwrap(), "simple");
        assert_eq!(expand("$(flavor NOPE)", &vars3).unwrap(), "undefined");

        // shell: runs the command, returns processed stdout.
        assert_eq!(e("$(shell echo hi)"), "hi");
        assert_eq!(e("$(shell printf 'a\\nb\\nc\\n')"), "a b c");
        assert_eq!(e("$(shell true)"), "");

        // error aborts (propagates as Err); info/warning expand to empty.
        assert!(expand("$(error boom)", &vars).is_err());
        assert_eq!(e("before$(info hi)after"), "beforeafter");

        // A computed name (`$($(F) …)`) is a variable reference, NOT a function call — the control
        // check must use the raw token, not the inline-expanded buffer (else it would panic).
        let vars4 = Vars::new([("F", "if")]);
        assert_eq!(expand("$($(F) 1,yes,no)", &vars4).unwrap(), "");
        assert_eq!(expand("${if 1,a,b}", &vars4).unwrap(), "a"); // brace form still works
    }

    #[test]
    fn test_function_edge_cases() {
        let vars = Vars::new([]);
        let e = |s: &str| expand(s, &vars).unwrap();

        // A nested call must not inherit the enclosing call's higher-numbered parameters.
        let nested = Vars::new([("inner", "[$(1)|$(2)]"), ("outer", "$(call inner,X)")]);
        assert_eq!(expand("$(call outer,A,B)", &nested).unwrap(), "[X|]");

        // A text function given too few arguments aborts, like GNU make.
        assert!(expand("$(subst a,b)", &vars).is_err());
        assert!(expand("$(word 1)", &vars).is_err());
        assert_eq!(e("$(subst a,b,xax)"), "xbx"); // enough args is fine

        // patsubst preserves TEXT's whitespace layout (unlike the other word functions).
        assert_eq!(e("$(patsubst a,X,a  b   c)"), "X  b   c");
        assert_eq!(e("$(patsubst z,X,a  b)"), "a  b");

        // A leading space before the name means it is NOT a function (matches GNU): `$( sort …)`
        // and `$( if …)` are undefined variable references.
        assert_eq!(e("$( sort b a)"), "");
        assert_eq!(e("$( if 1,a,b)"), "");

        // Argument comma-splitting balances only the opener's delimiter, so a stray `{` in a
        // `$(...)` call is literal (does not protect the following comma).
        assert_eq!(e("$(if 1,{a,b},c)"), "{a");

        // `$(call ...)` can invoke a built-in function by name.
        assert_eq!(e("$(call subst,a,b,xax)"), "xbx");
        assert_eq!(e("$(call if,1,yes,no)"), "yes");

        // `$(value)`/`$(flavor)` keep trailing whitespace in the name (so it stays unmatchable).
        let v = Vars::new([("FOO", "x")]);
        assert_eq!(expand("$(flavor FOO )", &v).unwrap(), "undefined");
        assert_eq!(expand("$(value FOO )", &v).unwrap(), "");
    }

    #[test]
    fn test_origin_and_eval() {
        let mut vars = Vars::new([]); // includes built-in defaults (CC, ...)
        vars.set_origin("ENVV", "x", false, Origin::Environment)
            .unwrap();
        vars.set("FILEV", "y", false).unwrap(); // File origin

        assert_eq!(expand("$(origin NOPE)", &vars).unwrap(), "undefined");
        assert_eq!(expand("$(origin CC)", &vars).unwrap(), "default");
        assert_eq!(expand("$(origin ENVV)", &vars).unwrap(), "environment");
        assert_eq!(expand("$(origin FILEV)", &vars).unwrap(), "file");
        // An automatic variable is `undefined` until bound; `automatic` once set (in a recipe).
        assert_eq!(expand("$(origin @)", &vars).unwrap(), "undefined");
        let mut recipe = vars.clone();
        recipe.set("@", "target.o", false).unwrap();
        assert_eq!(expand("$(origin @)", &recipe).unwrap(), "automatic");
        // Trailing whitespace makes the name unmatchable.
        assert_eq!(expand("$(origin FILEV )", &vars).unwrap(), "undefined");

        // `$(eval)` collects the expanded text (for the parser to evaluate) and expands to empty.
        let (result, evals) = expand_collect("[$(eval X := 1)]", &vars).unwrap();
        assert_eq!(result, "[]");
        assert_eq!(evals, vec!["X := 1".to_string()]);
    }

    #[test]
    fn test_path_and_file_functions() {
        let vars = Vars::new([]);
        let e = |s: &str| expand(s, &vars).unwrap();

        // `$(abspath)` is pure-lexical; absolute inputs are deterministic.
        assert_eq!(e("$(abspath /x/./y/../z)"), "/x/z");
        assert_eq!(e("$(abspath /a/b/ /c//d)"), "/a/b /c/d");

        // `$(file)`: write then read round-trips (a trailing newline is added on write and one is
        // stripped on read); append concatenates; an invalid operator errors.
        let path = std::env::temp_dir().join("omake_file_fn_test.txt");
        let p = path.to_string_lossy();
        assert_eq!(e(&format!("$(file > {p},hello)")), "");
        assert_eq!(e(&format!("$(file < {p})")), "hello");
        e(&format!("$(file >> {p},world)"));
        assert_eq!(e(&format!("$(file < {p})")), "hello\nworld");
        assert_eq!(e("$(file < /no/such/omake/file)"), "");
        assert!(expand("$(file bogus)", &vars).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
