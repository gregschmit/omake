// Backslash line continuation:
// - Non-recipe (variable value, prerequisite list) collapses `\`+newline into a single space.
// - Recipe keeps the `\`+newline (the shell continues the line) and strips the continuation line's
//   recipe prefix, so `echo one \` / `two` echoes as two lines but runs as `one two`.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "foo=a b c\nprereqs=d1 d2\necho one \\\ntwo\none two\n",
    expected_stderr: "",
    expected_files: &[],
});
