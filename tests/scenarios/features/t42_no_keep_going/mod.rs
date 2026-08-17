// `-S`/`--no-keep-going` turns off `-k`: here `-k -S` resolves to `-S` (last wins), so the build
// stops at the first failed target (`a`) instead of continuing to `b`, and exits 2. This is how a
// sub-make cancels a `-k` inherited from an ancestor via MAKEFLAGS. Matches gmake.
crate::system_test_cases!({
    args: &["-k", "-S"],
    expected_stdout: "a\n",
    expected_stderr: "omake: ERROR [Makefile:3] | Failed with code 1.\n  |\n3 | @echo a; false\n  | \n\n",
    expected_files: &[],
    expected_code: Some(2),
});
