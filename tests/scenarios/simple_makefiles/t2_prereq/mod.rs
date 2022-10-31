crate::system_test_cases!(
    {
        args: &[],
        expected_stdout: "echo \"This is a test\" > test\n",
        expected_stderr: "",
        expected_files: &[("test", "This is a test\n")],
    },
    {
        args: &["test_prereq"],
        expected_stdout: "?",
        expected_stderr: "",
        expected_files: &[("test", "This is a test\n"), ("test_prereq", "test2\n")],
    },
);
