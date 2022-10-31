use super::Vars;

/// Represents a frame on the stack inside the `expand` function. This is used for tracking the
/// previous buffer when expanding potentially nested expressions (i.e., either `$()` or `${}`).
/// Single variable expansions (e.g., `$X`) are handled inline without creating a frame since they
/// cannot possibly have nested expressions.
struct Frame {
    pub previous_buffer: String,
    /// Track which character opened this stack frame (should be parenthesis or brace).
    pub opening_delimiter: char,
}

/// The primary public interface for running variable expansion on an input string, given a
/// collection of `vars`.
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
                    None => current_buffer.push(c),
                    Some(f) => {
                        // Test if this character matches the opening delimiter.
                        if (c == '}' && f.opening_delimiter == '{')
                            || (c == ')' && f.opening_delimiter == '(')
                        {
                            // Expression terminated, so expand.
                            let var = vars.get(&current_buffer);
                            let recursive_result: String;

                            // Handle recursive variable expansion.
                            let result = if var.recursive {
                                recursive_result = expand(var.value.as_str(), vars)?;
                                &recursive_result
                            } else {
                                &var.value
                            };

                            // This `unwrap()` is safe because we checked that the stack contains a
                            // `last()` element, so it cannot be empty.
                            current_buffer = stack.pop().unwrap().previous_buffer;
                            current_buffer.push_str(result);
                            hit_variable = false;
                            continue;
                        }

                        // Not the right trailing delimiter, so consider it just a char.
                        current_buffer.push(c)
                    }
                }
            }
            _ => {
                // If we hit the variable indicator, then inline expansion since nesting is impossible.
                if hit_variable {
                    let eval = &vars.get(c).value;
                    current_buffer.push_str(eval);
                    hit_variable = false;
                    continue;
                }

                // Otherwise, just push the char.
                current_buffer.push(c);
            }
        }
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
}
