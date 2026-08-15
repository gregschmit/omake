// `.SILENT: hushed` suppresses echoing of hushed's recipe (only its output shows); `.IGNORE:
// tolerant` lets tolerant's failing `false` be non-fatal so the build continues.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "silent-cmd\nfalse\nrecovered\ndone\n",
    expected_stderr: "",
    expected_files: &[],
});
