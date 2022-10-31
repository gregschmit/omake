crate::system_test_cases!(
    {
        args: &["b"],
        expected_stdout: "echo b > b\n",
        expected_stderr: "",
        expected_files: &[("b", "b\n")],
        pre_hook: {
            std::fs::write("tests/scenarios/specific_features/t3_old_files/b", "b\n".to_string()).unwrap();
            std::fs::write("tests/scenarios/specific_features/t3_old_files/a", "a\n".to_string()).unwrap();
        },
    },
    {
        args: &["b", "-o", "b"],
        expected_stdout: "",
        expected_stderr: "make: INFO  | 'b' is up to date (old).\n",
        expected_files: &[("b", "b\n")],
        pre_hook: {
            std::fs::write("tests/scenarios/specific_features/t3_old_files/b", "b\n".to_string()).unwrap();
            std::fs::write("tests/scenarios/specific_features/t3_old_files/a", "a\n".to_string()).unwrap();
        },
    },
);
