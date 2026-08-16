crate::system_test_cases!(
    {
        // `-i` makes the failing `false` non-fatal, so the build continues to `after`; gmake still
        // prints the `Error N (ignored)` diagnostic to stderr.
        args: &["-i"],
        expected_stdout: "false\nafter\n",
        expected_stderr: "omake: [Makefile:2: all] Error 1 (ignored)\n",
        expected_files: &[],
    },
    {
        // Without `-i`, a failed recipe halts the build (and `after` never runs).
        args: &[],
        expected_stdout: "false\n",
        expected_stderr: "omake: ERROR [Makefile:2] | Failed with code 1.\n  |\n2 | false\n  | \n\n",
        expected_files: &[],
        expected_code: Some(2),
    },
);
