// Directives: `undefine` removes a variable (origin becomes undefined), `override` beats other
// assignments (and reports origin override), and a bare `export` exports every file variable to
// recipes (but not built-in defaults like CC). Cross-verified against gmake.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "gone=[undefined] locked=[winner](override)\nenv=[filevalue] []\n",
    expected_stderr: "",
    expected_files: &[],
});
