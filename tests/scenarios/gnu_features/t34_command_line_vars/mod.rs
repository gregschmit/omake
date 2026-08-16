// A command-line variable (`make all FOO=cmdval`) overrides the makefile's own assignment, reports
// origin `command line`, and is auto-exported to recipe environments. Cross-verified against gmake.
crate::system_test_cases!({
    args: &["all", "FOO=cmdval"],
    expected_stdout: "[cmdval] o=[command line] env=[cmdval]\n",
    expected_stderr: "",
    expected_files: &[],
});
