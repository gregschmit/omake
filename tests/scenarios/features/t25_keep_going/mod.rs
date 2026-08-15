// Without `-k`, a failed prerequisite (`a`) stops the build before `b`. With `-k`, the build
// continues to the unrelated `b`, but still exits non-zero. Error text on stderr is not pinned.
crate::system_test_cases!(
    {
        args: &[],
        expected_stdout: "",
        expected_stderr: "?",
        expected_files: &[],
        expected_code: Some(2),
    },
    {
        args: &["-k"],
        expected_stdout: "b-built\n",
        expected_stderr: "?",
        expected_files: &[],
        expected_code: Some(2),
    },
);
