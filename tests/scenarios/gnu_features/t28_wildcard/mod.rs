// Wildcard globbing (GNU): `$(wildcard *.c)` expands to sorted existing matches (empty for no
// match), a glob in a prerequisite list (`$(wildcard *.h)`) resolves to matching files, and a glob
// in an `include` operand (`conf*.mk`) pulls in the matching file. Cross-verified against gmake.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "srcs=[a.c b.c] hdrs=[util.h] conf=[vv] none=[]\n",
    expected_stderr: "",
    expected_files: &[],
});
