// Substitution references — the suffix form `$(SRCS:.c=.o)` and the pattern form
// `$(SRCS:%.c=%.pp)`. Both are shared by POSIX, GNU make, and BSD make.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "objs=foo.o bar.o plus=foo.pp bar.pp\n",
    expected_stderr: "",
    expected_files: &[],
});
