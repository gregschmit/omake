// A built-in implicit rule compiles `foo.o` from `foo.c` with no explicit rule in the makefile,
// using the default `COMPILE.c`/`OUTPUT_OPTION` variables. `-n` prints the command (matching gmake)
// without invoking the compiler.
crate::system_test_cases!({
    args: &["-n", "foo.o"],
    expected_stdout: "cc    -c -o foo.o foo.c\n",
    expected_stderr: "",
    expected_files: &[],
});
