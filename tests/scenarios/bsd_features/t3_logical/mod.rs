// BSD `.if` logical operators: `&&` binds tighter than `||`, parentheses group, `!` negates a
// primary, and comparisons bind tighter than both. A `&&`/`||` without surrounding whitespace stays
// part of a bare term (`1&&0` is true), matching bmake. Cross-verified against bmake.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "A=or_over_and B=paren C=chain D=neg_or E=bare\n",
    expected_stderr: "",
    expected_files: &[],
});
