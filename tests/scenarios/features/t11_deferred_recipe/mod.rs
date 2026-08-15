// Recipes are expanded at execution time:
// - `$(Q)` expands to `@`, which is then recognized as the silence modifier (so `hidden` prints
//   but the command is not echoed) — this only works if expansion happens before modifier parsing.
// - `$(LATER)` resolves even though LATER is defined after the rule.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "hidden\nval=defined-after\n",
    expected_stderr: "",
    expected_files: &[],
});
