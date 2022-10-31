crate::system_test_cases!(
    {
        args: &["b"],
        expected_stdout: "echo b > b\n",
        expected_stderr: "",
        // Don't add `b` otherwise it will be removed afterwards and it's a checked-in file.
        expected_files: &[],
        pre_hook: {
            std::fs::write("tests/scenarios/specific_features/t3_old_files/a", "a\n").unwrap();
        },
        post_hook: {
            std::fs::remove_file("tests/scenarios/specific_features/t3_old_files/a").unwrap();
        },
    },
    {
        args: &["b", "-o", "b"],
        expected_stdout: "",
        expected_stderr: "make: INFO  | 'b' is up to date (old).\n",
        expected_files: &[],
        pre_hook: {
            std::fs::write("tests/scenarios/specific_features/t3_old_files/a", "a\n").unwrap();
        },
        post_hook: {
            std::fs::remove_file("tests/scenarios/specific_features/t3_old_files/a").unwrap();
        },
    },
);
