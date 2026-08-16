// `-q`/`--question` runs no recipe and answers via the exit code: 1 when a target is out of date
// (a phony target with a normal recipe always is), 0 when up to date. A recipe of only `+` lines is
// an exception — the `+` line runs and the target counts as up to date (exit 0). Matches gmake.
crate::system_test_cases!(
    {
        // Phony `check` is always out of date; its normal recipe is not run, so no output, exit 1.
        args: &["-q", "check"],
        expected_stdout: "",
        expected_stderr: "",
        expected_files: &[],
        expected_code: Some(1),
    },
    {
        // `forced`'s recipe is only a `+` line, which still runs; the target is up to date, exit 0.
        args: &["-q", "forced"],
        expected_stdout: "forced-runs\n",
        expected_stderr: "",
        expected_files: &[],
        expected_code: Some(0),
    },
);
