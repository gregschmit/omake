// A shared prerequisite (`c`, depended on by both `a` and `b`) is built at most once per run, even
// though it appears twice in the graph. Without memoization `made-c` would print twice.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "made-c\nmade-a\nmade-b\n",
    expected_stderr: "",
    expected_files: &[],
});
