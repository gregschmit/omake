// `-r`/`--no-builtin-rules` disables the built-in implicit rules but keeps the built-in variables
// (`CC` is still `cc`). `-R`/`--no-builtin-variables` additionally undefines the built-in variables
// (`CC`/`COMPILE.c` are empty, `$(origin CC)` is `undefined`) and implies `-r`. Matches gmake.
crate::system_test_cases!(
    {
        args: &["-r"],
        expected_stdout: "CC=[cc] origin=[default] COMPILE.c=[cc    -c]\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["-R"],
        expected_stdout: "CC=[] origin=[undefined] COMPILE.c=[]\n",
        expected_stderr: "",
        expected_files: &[],
    },
    {
        args: &["--no-builtin-variables"],
        expected_stdout: "CC=[] origin=[undefined] COMPILE.c=[]\n",
        expected_stderr: "",
        expected_files: &[],
    },
);
