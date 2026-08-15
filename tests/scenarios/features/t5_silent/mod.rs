// The `@` modifier suppresses echoing of the command (but its output still shows); a plain line is
// echoed before running.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "silent output\necho visible\nvisible\n",
    expected_stderr: "",
    expected_files: &[],
});
