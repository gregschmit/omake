// `-I DIR`/`--include-dir DIR` adds DIR to the include search path: `include shared.mk` is not in
// the current directory but is found in `inc/` because of `-I inc`. The current directory is still
// searched first. Matches gmake.
crate::system_test_cases!({
    args: &["-I", "inc"],
    expected_stdout: "VAR=[from-include-dir]\n",
    expected_stderr: "",
    expected_files: &[],
});
