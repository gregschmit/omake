// Inline `#` comments are stripped from assignments and rule lines: `CC` is `gcc` (not
// `gcc# ...`), and `all`'s only prerequisite is `dep` (not `dep`, `#`, `trailing`, `comment`).
crate::system_test_cases!({
    args: &[],
    expected_stdout: "made-dep\ncc-is-[gcc]\n",
    expected_stderr: "",
    expected_files: &[],
});
