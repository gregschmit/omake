// `-t`/`--touch` marks out-of-date targets up to date by touching (creating/bumping) their files
// instead of running recipes, echoing `touch <target>` to stdout. Prerequisites are resolved first
// (`bar` before `foo`). Matches gmake. The created files are cleaned up by the harness `Drop`.
crate::system_test_cases!({
    args: &["-t"],
    expected_stdout: "touch bar\ntouch foo\n",
    expected_stderr: "",
    expected_files: &[("bar", ""), ("foo", "")],
    pre_hook: {
        let _ = std::fs::remove_file(format!("{path}/foo"));
        let _ = std::fs::remove_file(format!("{path}/bar"));
    },
});
