crate::system_test_cases!(
    {
        args: &[],
        expected_stdout: "echo \"test\"\ntest\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["-n"],
        expected_stdout: "echo \"test\"\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["--just-print"],
        expected_stdout: "echo \"test\"\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["--dry-run"],
        expected_stdout: "echo \"test\"\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["--recon"],
        expected_stdout: "echo \"test\"\n",
        expected_stderr: "",
        expected_files: &[],
    },
);
