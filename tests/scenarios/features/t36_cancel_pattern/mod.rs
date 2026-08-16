// A pattern rule with no recipe cancels an earlier one (BSD make style; also GNU make for `%`
// patterns): the second `%.o: %.c` removes the compile rule, so `foo.o` can no longer be made and
// the build errors instead of running the cancelled recipe.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "",
    expected_stderr: "omake: ERROR | No rule to make target 'foo.o'.\n",
    expected_files: &[],
    expected_code: Some(2),
    pre_hook: {
        std::fs::write("tests/scenarios/features/t36_cancel_pattern/foo.c", "").unwrap();
    },
    post_hook: {
        let _ = std::fs::remove_file("tests/scenarios/features/t36_cancel_pattern/foo.c");
    },
});
