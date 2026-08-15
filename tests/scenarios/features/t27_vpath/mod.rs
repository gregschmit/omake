// `VPATH` makes make find `foo.c` in `src/`; the resolved path (`src/foo.c`) is what `$<`/`$^` use.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "found: src/foo.c all: src/foo.c\n",
    expected_stderr: "",
    expected_files: &[],
});
