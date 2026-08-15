// `include config.mk` pulls in a variable definition from another file before it is used.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "greeting=hello-from-include\n",
    expected_stderr: "",
    expected_files: &[],
});
