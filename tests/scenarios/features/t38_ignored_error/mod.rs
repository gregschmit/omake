// A `-` prefix on a recipe line makes its failure non-fatal: the build continues, exits 0, and
// gmake prints `<prog>: [file:line: target] Error N (ignored)` to stderr with the failing command's
// exit code. Matches gmake.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "exit 3\nrecovered\n",
    expected_stderr: "omake: [Makefile:2: all] Error 3 (ignored)\n",
    expected_files: &[],
});
