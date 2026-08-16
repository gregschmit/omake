// `.SHELLSTATUS` holds the exit code of the most recent `$(shell ...)`, updated both at makefile
// level (the `:=` assignment) and between recipe lines. A `$(shell exit 7)` on one recipe line is
// visible as `.SHELLSTATUS` on the next. Matches gmake.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "makefile-level=[4]\nrecipe-ran\nrecipe-level=[7]\n",
    expected_stderr: "",
    expected_files: &[],
});
