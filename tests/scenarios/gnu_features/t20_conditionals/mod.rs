// Conditional directives: `ifeq`/`else`/`endif` selects MODE by the value of V, and `ifdef`/`else`
// selects X by whether UNSET is defined (it isn't).
crate::system_test_cases!({
    args: &[],
    expected_stdout: "mode=optimized x=undefined\n",
    expected_stderr: "",
    expected_files: &[],
});
