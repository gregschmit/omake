// `-j`/`--jobs[=N]` is accepted (a bare `make -jN` no longer errors). omake currently builds
// serially, which is a valid serialization; recipe lines within one target always run in order, so
// this output matches gmake regardless of the job count.
crate::system_test_cases!({
    args: &["-j4"],
    expected_stdout: "one\ntwo\n",
    expected_stderr: "",
    expected_files: &[],
});
