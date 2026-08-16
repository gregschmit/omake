crate::system_test_cases!(
    {
        args: &["b"],
        expected_stdout: "echo b > b\n",
        expected_stderr: "",
        // Don't add `b` otherwise it will be removed afterwards and it's a checked-in file.
        expected_files: &[],
        pre_hook: {
            std::fs::write("tests/scenarios/features/t3_old_files/a", "a\n").unwrap();
        },
        post_hook: {
            // Cleanup should never panic over the real assertion, so ignore errors.
            let _ = std::fs::remove_file("tests/scenarios/features/t3_old_files/a");
        },
    },
    {
        args: &["b", "-o", "b"],
        expected_stdout: "",
        expected_stderr: "omake: INFO  | Target 'b' is up to date (old).\n",
        expected_files: &[],
        pre_hook: {
            std::fs::write("tests/scenarios/features/t3_old_files/a", "a\n").unwrap();
        },
        post_hook: {
            // Cleanup should never panic over the real assertion, so ignore errors.
            let _ = std::fs::remove_file("tests/scenarios/features/t3_old_files/a");
        },
    },
);
