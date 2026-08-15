// A `define ... endef` multi-line variable used as a canned recipe: each body line becomes its own
// command (echoed then run), so output interleaves the echoed commands with their output.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "echo hello\nhello\necho world\nworld\n",
    expected_stderr: "",
    expected_files: &[],
});
