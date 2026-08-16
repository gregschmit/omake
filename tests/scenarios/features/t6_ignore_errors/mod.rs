crate::system_test_cases!(
    {
        // `-i` makes the failing `false` non-fatal, so the build continues to `after`.
        args: &["-i"],
        expected_stdout: "false\nafter\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        // Without `-i`, a failed recipe halts the build (and `after` never runs).
        args: &[],
        expected_stdout: "false\n",
        expected_stderr: "omake: ERROR [Makefile:1] | Failed with code 1.\n  |\n1 | all:\n  | \n\n",
        expected_files: &[],
        expected_code: Some(2),
    },
);
