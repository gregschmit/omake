crate::system_test_cases!(
    {
        args: &[],
        expected_stdout: "",
        expected_stderr: "omake: ERROR [Makefile:2] | Invalid line type.\n  |\n2 |   echo \"bad indentation on this line is intentional\" > a\n  | \n\n",
        expected_files: &[],
        expected_code: Some(2),
    },
);
