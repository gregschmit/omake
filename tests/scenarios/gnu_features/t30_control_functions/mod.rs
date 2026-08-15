// GNU control/shell functions composed as in a real makefile: `foreach`+`call` build a list,
// `if`/`or`/`and` select values, `shell` runs a command, and `words` is used inside a `foreach`
// body. Cross-verified against gmake.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "loop=[hello-alpha hello-beta hello-gamma]\npick=[on] [off]\nshell=[ABC]\nor=[fallback] and=[z]\nwords=[alpha:3 beta:3 gamma:3]\n",
    expected_stderr: "",
    expected_files: &[],
});
