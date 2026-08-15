// `export` makes a make variable visible in a recipe's shell environment (`$GREETING`), while an
// unexported variable is not present there.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "greeting=hello local=[]\n",
    expected_stderr: "",
    expected_files: &[],
});
