//! Implementation of variable expansion.

use crate::vars::Vars;

/// A stack frame used to track the previous buffer when expanding potentially nested expressions
/// (i.e., either `$()` or `${}`). Single variable expansions (e.g., `$X`) are handled inline
/// without creating a frame since they cannot possibly have nested expressions.
struct Frame {
    pub previous_buffer: String,
    /// Which character opened this stack frame (parenthesis or brace)?
    pub opening_delimiter: char,
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

    for c in s.chars() {
        match c {
            '$' => {
                hit_variable = !hit_variable;

                // Push a literal `$` if it's the second one (`hit_variable` is `false`)
                if !hit_variable {
                    current_buffer.push(c);
                }
            }
            '(' | '{' => {
                // If we haven't hit a variable, consider this a normal char.
                if !hit_variable {
                    current_buffer.push(c);
                    continue;
                }

                // Otherwise, push a frame onto the stack to begin processing this expression.
                stack.push(Frame {
                    previous_buffer: current_buffer,
                    opening_delimiter: c,
                });
                current_buffer = "".to_string();
                hit_variable = false;
            }
            ')' | '}' => {
                match stack.last() {
                    None => {
                        current_buffer.push(c);
                        hit_variable = false;
                    }
                    Some(f) => {
                        // Test if this character matches the opening delimiter.
                        if (c == '}' && f.opening_delimiter == '{')
                            || (c == ')' && f.opening_delimiter == '(')
                        {
                            // Expression terminated, so expand. A substitution reference is
                            // `NAME:FROM=TO` (a `:` is otherwise an invalid character in a name, so
                            // it only appears here for a substitution). `FROM=TO` supports both the
                            // suffix form (`.c=.o`) and the pattern form (`%.c=%.o`).
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
                                    return Err(format!(
                                        "Recursive variable '{name}' references itself."
                                    ));
                                }
                                expanding.push(name.clone());
                                recursive_result =
                                    expand_inner(var.value.as_str(), vars, expanding)?;
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

                            // This `unwrap()` is safe because we checked that the stack contains a
                            // `last()` element, so it cannot be empty.
                            current_buffer = stack.pop().unwrap().previous_buffer;
                            current_buffer.push_str(result);
                            hit_variable = false;
                            continue;
                        }

                        // Not the right trailing delimiter, so consider it just a char.
                        current_buffer.push(c);
                        hit_variable = false;
                    }
                }
            }
            _ => {
                // If we hit the variable indicator, then inline expansion since nesting is impossible.
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

/// Apply a substitution reference (`$(VAR:FROM=TO)`) to each whitespace-separated word of `value`.
fn substitute_ref(value: &str, from: &str, to: &str) -> String {
    value
        .split_whitespace()
        .map(|word| substitute_word(word, from, to))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Substitute one word. If `from` contains `%`, it is a pattern (prefix `%` suffix) whose stem is
/// substituted into `to`'s `%`; otherwise `from` is a suffix replaced by `to`. A word that does not
/// match is returned unchanged.
fn substitute_word(word: &str, from: &str, to: &str) -> String {
    match from.find('%') {
        Some(pct) => {
            let (prefix, suffix) = (&from[..pct], &from[pct + 1..]);
            if word.len() >= prefix.len() + suffix.len()
                && word.starts_with(prefix)
                && word.ends_with(suffix)
            {
                let stem = &word[prefix.len()..word.len() - suffix.len()];
                match to.find('%') {
                    Some(tpct) => format!("{}{stem}{}", &to[..tpct], &to[tpct + 1..]),
                    None => to.to_string(),
                }
            } else {
                word.to_string()
            }
        }
        None => match word.strip_suffix(from) {
            Some(prefix) => format!("{prefix}{to}"),
            None => word.to_string(),
        },
    }
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
}
