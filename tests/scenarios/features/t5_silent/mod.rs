// The `@` modifier suppresses echoing of the command (but its output still shows); a plain line is
// echoed before running. The `-s`/`--silent`/`--quiet` flag silences echoing for every recipe line
// (like a global `.SILENT:`), so even the plain line is not echoed.
crate::system_test_cases!(
    {
        args: &[],
        expected_stdout: "silent output\necho visible\nvisible\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["-s"],
        expected_stdout: "silent output\nvisible\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["--silent"],
        expected_stdout: "silent output\nvisible\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["--quiet"],
        expected_stdout: "silent output\nvisible\n",
        expected_stderr: "",
        expected_files: &[],
    },
);
