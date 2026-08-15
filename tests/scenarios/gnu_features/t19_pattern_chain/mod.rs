// Chained pattern rules: building `foo.o` needs `foo.c`, which no file provides but the `%.c: %.y`
// rule can produce from `foo.y` (created by the pre_hook). The chain `%.y -> %.c -> %.o` resolves.
crate::system_test_cases!({
    args: &["foo.o"],
    expected_stdout: "yacc foo.y -o foo.c\ncc foo.c -o foo.o\n",
    expected_stderr: "",
    expected_files: &[],
    pre_hook: {
        std::fs::write("tests/scenarios/gnu_features/t19_pattern_chain/foo.y", "").unwrap();
    },
    post_hook: {
        let _ = std::fs::remove_file("tests/scenarios/gnu_features/t19_pattern_chain/foo.y");
    },
});
