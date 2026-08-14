crate::system_test_cases!({
    args: &["nonexistent"],
    expected_stdout: "",
    expected_stderr: "make: ERROR | No rule to make target 'nonexistent'.\n",
    expected_files: &[],
    expected_code: Some(2),
});
