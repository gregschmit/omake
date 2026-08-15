crate::system_test_cases!(
    {
        args: &[],
        expected_stdout: "echo \"This is a test\"\nThis is a test\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["test"],
        expected_stdout: "echo \"This is a test\"\nThis is a test\n",
        expected_stderr: "",
        expected_files: &[],
    },
);
