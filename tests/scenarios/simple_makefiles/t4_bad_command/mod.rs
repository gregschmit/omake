crate::system_test_cases!({
    args: &[],
    expected_stdout: "somepresumeablybadcommand\n",
    // The shell's "command not found" wording is platform-specific, so don't pin stderr; the
    // non-zero exit code is the signal that a failed recipe halts the build.
    expected_stderr: "?",
    expected_files: &[],
    expected_code: Some(2),
});
