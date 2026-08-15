// `$(MAKE)` expands to the omake invocation path, so a recipe can recurse into a subdirectory.
// The chdir INFO messages on stderr contain machine-specific absolute paths, so stderr is skipped.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "in-sub-dir\n",
    expected_stderr: "?",
    expected_files: &[],
    expected_code: Some(0),
});
