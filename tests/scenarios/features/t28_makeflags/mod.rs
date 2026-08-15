// `MAKEFLAGS` reflects the active single-letter flags (empty, or `k` with `-k`), and `MAKELEVEL` is
// 0 at the top level. (The harness clears inherited MAKEFLAGS/MAKELEVEL for determinism.)
crate::system_test_cases!(
    {
        args: &[],
        expected_stdout: "flags=[] level=[0]\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["-k"],
        expected_stdout: "flags=[k] level=[0]\n",
        expected_stderr: "",
        expected_files: &[],
    },
);
