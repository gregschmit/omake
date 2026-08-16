// A `define ... endef` multi-line variable used as a canned recipe: each body line becomes its own
// command (echoed then run), so output interleaves the echoed commands with their output. A `@` in
// front of the reference (`@$(GREET)`) is a line-level modifier and silences *every* expanded line.
crate::system_test_cases!(
    {
        args: &["all"],
        expected_stdout: "echo hello\nhello\necho world\nworld\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["quiet"],
        expected_stdout: "hello\nworld\n",
        expected_stderr: "",
        expected_files: &[],
    },
);
