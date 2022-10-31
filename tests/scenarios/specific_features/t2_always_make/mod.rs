const OUTPUT: &str = "echo test > a
echo test > test\n";

crate::system_test_cases!({
    args: &["-B", "test"],
    expected_stdout: OUTPUT,
    expected_stderr: "",
    expected_files: &[("test", "test\n")],
});
