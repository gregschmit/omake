// A pattern rule `%.o: %.c` builds `foo.o` from `foo.c` (created by the pre_hook), with the stem
// (`foo`) available as `$*` and the concrete target/prereq as `$@`/`$<`.
crate::system_test_cases!({
    args: &["foo.o"],
    expected_stdout: "compiling foo.c to foo.o stem=foo\n",
    expected_stderr: "",
    expected_files: &[],
    pre_hook: {
        std::fs::write("tests/scenarios/features/t18_pattern_rule/foo.c", "").unwrap();
    },
    post_hook: {
        let _ = std::fs::remove_file("tests/scenarios/features/t18_pattern_rule/foo.c");
    },
});
