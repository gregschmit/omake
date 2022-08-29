use super::MakeError;
use super::VarMap;

/// Represents a frame on the stack inside the `expand` function. This is used for tracking the
/// previous buffer when expanding potentially nested expressions (i.e., either `$()` or `${}`).
/// Single variable expansions (e.g., `$X`) are handled inline without creating a frame since they
/// cannot possibly have nested expressions.
struct Frame {
    pub previous_buffer: String,
    /// Track if the frame buffer ended with a brace (otherwise assume parenthesis).
    pub brace: bool,
}

/// The primary public interface for running variable expansion on an input string, given a hash
/// of `vars`.
///
/// The goal here is to be `O(n)`. This works by iterating over the input string and storing plain
/// characters into a buffer until we hit either:
///  1. A simple variable expansion (e.g., `$X`), where we just evaluate it against `vars` inline
///     since there could not possibly be any nesting.
///  2. A long variable expansion (e.g., `$(` or `${`), where there could be nested
///     expressions, where we push the current buffer onto a stack, and then continue parsing. When
///     we hit a matching closing delimiter (tracked on the stack frame), we evaluate the buffer,
///     pop the previous buffer off the stack, join it with the evaluated value, and keep going.
pub fn expand<S: Into<String>>(s: S, vars: &VarMap) -> Result<String, MakeError> {
    let s = s.into();
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
                    brace: c == '{',
                });
                current_buffer = "".to_string();
                hit_variable = false;
            }
            ')' | '}' => {
                match stack.last() {
                    None => current_buffer.push(c),
                    Some(f) => {
                        if f.brace == (c == '}') {
                            // Expression terminated, so expand.
                            let var = vars.get(&current_buffer);
                            let recursive_result: String;

                            // Handle recursive variable expansion.
                            let result = if var.recursive {
                                recursive_result = expand(&var.value, vars)?;
                                &recursive_result
                            } else {
                                &var.value
                            };

                            current_buffer = stack.pop().unwrap().previous_buffer;
                            current_buffer.push_str(result);
                            hit_variable = false;
                            continue;
                        }

                        // Not the right trailing brace, so consider it just a char.
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

    Ok(current_buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_single_letter_expansions() {
        let vars = VarMap::new([("A", "VALUE A"), ("B", "VALUE B")]);
        assert_eq!(expand("$A", &vars).unwrap(), "VALUE A",);
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
        let vars = VarMap::new([("TESTA", "VALUE A"), ("TESTB", "VALUE B")]);
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
        let vars = VarMap::new([("A", "B"), ("B", "VALUE1"), ("CD", "VALUE2"), ("E", "D")]);
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
            expand("This is $(${A})!", &vars).unwrap(),
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
        let vars = VarMap::new([("A", "B")]);
        assert_eq!(expand("This is $$A!", &vars).unwrap(), "This is $A!",);
        assert_eq!(expand("This is $${A}!", &vars).unwrap(), "This is ${A}!",);
        assert_eq!(expand("This is $$${A}!", &vars).unwrap(), "This is $B!",);
    }

    #[test]
    fn test_not_recursive() {
        let vars = VarMap::new([("A", "B"), ("C", "${A}")]);
        assert_eq!(expand("Test ${C}", &vars).unwrap(), "Test ${A}");
    }

    #[test]
    fn test_recursive() {
        let mut vars = VarMap::new([("A", "B"), ("C", "${A}")]);
        let mut recursive_var = vars.map.get_mut("C").unwrap();
        recursive_var.recursive = true;
        assert_eq!(expand("Test ${C}", &vars).unwrap(), "Test B");
    }

    #[test]
    fn test_double_recursive() {
        let mut vars = VarMap::new([("A", "B"), ("C", "${A}"), ("D", "$(C)")]);
        for ch in ["C", "D"] {
            let mut var = vars.map.get_mut(ch).unwrap();
            var.recursive = true;
        }
        assert_eq!(expand("Test ${D}", &vars).unwrap(), "Test B");
    }

    #[test]
    fn test_intermediate_not_recursive() {
        let mut vars = VarMap::new([("A", "B"), ("C", "${A}"), ("D", "$(C)")]);
        for ch in ["A", "D"] {
            let mut var = vars.map.get_mut(ch).unwrap();
            var.recursive = true;
        }
        assert_eq!(expand("Test ${D}", &vars).unwrap(), "Test ${A}");
    }

    #[test]
    fn test_recursion_on_simple_value_works() {
        let mut vars = VarMap::new([("A", "B"), ("C", "${A}"), ("D", "$(C)")]);
        for ch in ["A", "C", "D"] {
            let mut var = vars.map.get_mut(ch).unwrap();
            var.recursive = true;
        }
        assert_eq!(expand("Test ${D}", &vars).unwrap(), "Test B");
    }
}
