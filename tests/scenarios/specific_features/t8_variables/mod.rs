// Variable assignment and expansion via both `$(...)` and `${...}` inside recipes.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "Hello, world!\nworld\n",
    expected_stderr: "",
    expected_files: &[],
});
