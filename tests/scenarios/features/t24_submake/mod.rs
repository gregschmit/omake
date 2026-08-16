// `$(MAKE)` expands to the omake invocation path, so a recipe can recurse into a subdirectory.
// `--no-print-directory` suppresses the sub-make's "Entering/Leaving directory" lines (which carry
// machine-specific absolute paths), leaving just the sub-make's own output.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "in-sub-dir\n",
    expected_stderr: "",
    expected_files: &[],
    expected_code: Some(0),
});
