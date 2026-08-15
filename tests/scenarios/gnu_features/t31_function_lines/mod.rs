// Parser-level function behavior: a standalone `$(info ...)` line prints at parse time; a `#`
// inside a `$(...)` reference is literal (not a comment); and `$(foreach)`+`$(call)` compose. The
// info line prints before the recipe output. Cross-verified against gmake.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "configuring\nurl=[http://x#frag]\npairs=[a-z b-z]\nshell=[has#hash]\n",
    expected_stderr: "",
    expected_files: &[],
});
