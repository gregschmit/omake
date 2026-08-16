// BSD `.warning MESSAGE` prints a (variable-expanded) warning to stderr and continues the build,
// like bmake. `.error` aborts and `.info` is a plain message (same dispatch).
crate::system_test_cases!({
    args: &[],
    expected_stdout: "done\n",
    expected_stderr: "omake: WARN  [Makefile:2] | building in debug mode\n  |\n2 | .warning building in ${MODE} mode\n  | \n\n",
    expected_files: &[],
});
