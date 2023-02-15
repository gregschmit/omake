crate::system_test_cases!(
    {
        args: &[],
        expected_stdout: "",
        expected_stderr: "make: ERROR [Makefile] | Invalid line type.\n  |\n2 |   echo \"bad indentation on this line is intentional\" > a\n  | \n\n",
        expected_files: &[],
    },
);
