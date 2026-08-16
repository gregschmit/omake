// A static pattern rule `$(objs): %.o: %.c` builds each listed target from its own stem-derived
// prerequisite: for `a.o` the stem is `a`, so `$<` is `a.c`, `$*` is `a`, and `$^` is `a.c`. The
// pre_hook creates the `.c` sources (the recipe only echoes, so no `.o` files are produced).
crate::system_test_cases!({
    args: &[],
    expected_stdout: "a.o from a.c (stem a, all a.c)\nb.o from b.c (stem b, all b.c)\n",
    expected_stderr: "",
    expected_files: &[],
    pre_hook: {
        let dir = "tests/scenarios/gnu_features/t36_static_pattern";
        std::fs::write(format!("{dir}/a.c"), "").unwrap();
        std::fs::write(format!("{dir}/b.c"), "").unwrap();
    },
    post_hook: {
        let dir = "tests/scenarios/gnu_features/t36_static_pattern";
        let _ = std::fs::remove_file(format!("{dir}/a.c"));
        let _ = std::fs::remove_file(format!("{dir}/b.c"));
    },
});
