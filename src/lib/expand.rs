//! Implementation of variable expansion.

use crate::vars::Vars;

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
    /// close is not mistaken for the end of the expression (`$(subst (a,b),Z,text)`).
    pub paren_depth: u32,
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
    expand_inner(s, vars, &mut Vec::new())
}

/// Inner expansion that carries `expanding`: the names of recursive variables currently being
/// expanded. This lets us reject a variable that references itself (directly or through a chain)
/// instead of recursing until the stack overflows.
fn expand_inner(s: &str, vars: &Vars, expanding: &mut Vec<String>) -> Result<String, String> {
    let mut stack: Vec<Frame> = vec![];
    let mut current_buffer: String = String::with_capacity(s.len());
    let mut hit_variable: bool = false;

    for (i, c) in s.char_indices() {
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
                let raw = &s[f.raw_start..i];

                // A function call is `$(name args)`: a known function name followed by whitespace.
                // Each argument is split from the raw text (commas inside `$(...)` / `(...)` are
                // protected) and expanded independently. Unknown names fall through to a variable.
                if let Some((func, raw_args)) = raw.trim_start().split_once(char::is_whitespace)
                    && let Some(arity) = function_arity(func)
                {
                    let mut args = Vec::with_capacity(arity);
                    for part in split_function_args(raw_args.trim_start(), arity) {
                        args.push(expand_inner(part, vars, expanding)?);
                    }
                    let result = call_function(func, &args)?;
                    current_buffer = stack.pop().unwrap().previous_buffer;
                    current_buffer.push_str(&result);
                    hit_variable = false;
                    continue;
                }

                // A substitution reference is `NAME:FROM=TO` (a `:` is otherwise an invalid
                // character in a name, so it only appears here for a substitution). `FROM=TO`
                // supports both the suffix form (`.c=.o`) and the pattern form (`%.c=%.o`). The
                // name uses the inline-expanded buffer (so computed names like `$($(X))` resolve).
                let content = current_buffer.trim();
                let (name, subst) = match content.split_once(':') {
                    Some((n, s)) if s.contains('=') => {
                        let (from, to) = s.split_once('=').unwrap();
                        (
                            n.trim().to_string(),
                            Some((from.to_string(), to.to_string())),
                        )
                    }
                    _ => (content.to_string(), None),
                };

                let var = vars.get(&name);
                let recursive_result: String;

                // Handle recursive variable expansion, guarding against a cycle.
                let value = if var.recursive {
                    if expanding.contains(&name) {
                        return Err(format!("Recursive variable '{name}' references itself."));
                    }
                    expanding.push(name.clone());
                    recursive_result = expand_inner(var.value.as_str(), vars, expanding)?;
                    expanding.pop();
                    &recursive_result
                } else {
                    &var.value
                };

                // Apply the substitution reference to the resolved value, if present.
                let substituted: String;
                let result: &str = match &subst {
                    Some((from, to)) => {
                        substituted = substitute_ref(value, from, to);
                        &substituted
                    }
                    None => value,
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
        | "basename" | "suffix" => 1,
        _ => return None,
    })
}

/// Dispatch a make function on its already-split, already-expanded arguments. `name` must be a
/// known function (see `function_arity`). Errors on invalid input (e.g. a bad `word` index).
fn call_function(name: &str, args: &[String]) -> Result<String, String> {
    Ok(match name {
        "word" => return func_word(args),
        "wordlist" => return func_wordlist(args),
        "wildcard" => wildcard(arg(args, 0)),
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

/// Apply a substitution reference (`$(VAR:FROM=TO)`) to each whitespace-separated word of `value`.
fn substitute_ref(value: &str, from: &str, to: &str) -> String {
    value
        .split_whitespace()
        .map(|word| substitute_word(word, from, to))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Substitute one word for a substitution reference. If `from` contains `%`, it is a pattern whose
/// stem is substituted into `to`'s `%`; otherwise `from` is a suffix replaced by `to`. A word that
/// does not match is returned unchanged.
fn substitute_word(word: &str, from: &str, to: &str) -> String {
    if from.contains('%') {
        match pattern_stem(from, word) {
            Some(stem) => subst_stem(to, stem),
            None => word.to_string(),
        }
    } else {
        match word.strip_suffix(from) {
            Some(prefix) => format!("{prefix}{to}"),
            None => word.to_string(),
        }
    }
}

/// If `word` matches the `%` pattern `pat` (which must contain a `%`), return the matched stem.
fn pattern_stem<'a>(pat: &str, word: &'a str) -> Option<&'a str> {
    let pct = pat.find('%')?;
    let (prefix, suffix) = (&pat[..pct], &pat[pct + 1..]);
    if word.len() >= prefix.len() + suffix.len()
        && word.starts_with(prefix)
        && word.ends_with(suffix)
    {
        Some(&word[prefix.len()..word.len() - suffix.len()])
    } else {
        None
    }
}

/// Substitute `stem` into the `%` of `to` (or return `to` unchanged when it has no `%`).
fn subst_stem(to: &str, stem: &str) -> String {
    match to.find('%') {
        Some(tpct) => format!("{}{stem}{}", &to[..tpct], &to[tpct + 1..]),
        None => to.to_string(),
    }
}

/// Whether `word` matches `pattern`: a `%` pattern (any stem) or, without `%`, an exact match.
fn word_matches(pattern: &str, word: &str) -> bool {
    if pattern.contains('%') {
        pattern_stem(pattern, word).is_some()
    } else {
        pattern == word
    }
}

/// Split `s` into at most `max` comma-separated arguments, ignoring commas nested inside
/// `(...)`/`{...}`. The final argument keeps any remaining commas (matching GNU make).
fn split_function_args(s: &str, max: usize) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if args.len() + 1 == max {
            break;
        }
        match c {
            '(' | '{' => depth += 1,
            ')' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
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
/// an exact word) by `REPLACEMENT`.
fn func_patsubst(args: &[String]) -> String {
    let (pattern, replacement, text) = (arg(args, 0), arg(args, 1), arg(args, 2));
    map_words(text, |word| {
        if pattern.contains('%') {
            match pattern_stem(pattern, word) {
                Some(stem) => subst_stem(replacement, stem),
                None => word.to_string(),
            }
        } else if word == pattern {
            replacement.to_string()
        } else {
            word.to_string()
        }
    })
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
}
