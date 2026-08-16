// `.SILENT: hushed` suppresses echoing of hushed's recipe (only its output shows); `.IGNORE:
// tolerant` lets tolerant's failing `false` be non-fatal so the build continues. gmake still prints
// the `[file:line: target] Error N (ignored)` diagnostic to stderr (a target-specific `.SILENT`
// does not suppress it).
crate::system_test_cases!({
    args: &[],
    expected_stdout: "silent-cmd\nfalse\nrecovered\ndone\n",
    expected_stderr: "omake: [Makefile:8: tolerant] Error 1 (ignored)\n",
    expected_files: &[],
});
