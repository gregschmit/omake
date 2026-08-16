// A recipe-less explicit rule (`app.o: app.h`) borrows the recipe of a matching pattern rule
// (`%.o: %.c`): the pattern-derived prerequisite (`app.c`) comes first, then the explicit one
// (`app.h`), so `$<` is `app.c` and `$^` is `app.c app.h` — matching GNU make.
crate::system_test_cases!({
    args: &["app.o"],
    expected_stdout: "compiling app.c -> app.o (deps: app.c app.h)\n",
    expected_stderr: "",
    expected_files: &[],
    pre_hook: {
        let dir = "tests/scenarios/gnu_features/t37_borrow_recipe";
        std::fs::write(format!("{dir}/app.c"), "").unwrap();
        std::fs::write(format!("{dir}/app.h"), "").unwrap();
    },
    post_hook: {
        let dir = "tests/scenarios/gnu_features/t37_borrow_recipe";
        let _ = std::fs::remove_file(format!("{dir}/app.c"));
        let _ = std::fs::remove_file(format!("{dir}/app.h"));
    },
});
