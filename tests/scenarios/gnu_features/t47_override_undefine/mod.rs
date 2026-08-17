// `override undefine` of a command-line variable does not fully remove it: gmake keeps a "residual
// environment copy", so the value stays, `$(origin)` drops from "command line" to "environment",
// and it is still exported to the recipe's environment. Matches gmake.
crate::system_test_cases!({
    args: &["V=fromcmd"],
    expected_stdout: "V=[fromcmd] origin=[environment] env=[fromcmd]\n",
    expected_stderr: "",
    expected_files: &[],
});
