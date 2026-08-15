// A POSIX suffix rule `.c.o:` (shared by GNU/BSD) builds `foo.o` from `foo.c`, translated to the
// pattern rule `%.o: %.c`; `$<`/`$@`/`$*` resolve to `foo.c`/`foo.o`/`foo`.
crate::system_test_cases!({
    args: &["foo.o"],
    expected_stdout: "cc foo.c -o foo.o stem=foo\n",
    expected_stderr: "",
    expected_files: &[],
});
