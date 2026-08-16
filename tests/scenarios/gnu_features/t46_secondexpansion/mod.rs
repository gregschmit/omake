// `.SECONDEXPANSION:` makes the prerequisites of following rules expand a second time at build
// time, with `$@` (and `$*`) in scope. A `$$@` prerequisite becomes the target name, so `foo`
// depends on `bar-foo` (and `baz`), which are built first. Matches gmake.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "built bar-foo\nbuilt baz\nmaking foo from prereqs: bar-foo baz\n",
    expected_stderr: "",
    expected_files: &[],
});
