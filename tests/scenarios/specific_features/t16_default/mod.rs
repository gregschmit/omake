// `.DEFAULT` provides a recipe for `foo`, which has no rule of its own; `$@` is the actual target.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "default-for foo\nbuilt-all\n",
    expected_stderr: "",
    expected_files: &[],
});
