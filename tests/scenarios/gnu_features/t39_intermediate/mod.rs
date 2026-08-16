// A chained implicit rule (`%.o: %.c` + `%.c: %.y`) builds `prog.o` from `prog.y`. The invented
// `prog.c` is an intermediate: it is created to link the chain, then deleted afterward (the `rm`
// line), matching GNU make keeping the tree clean of chained-rule byproducts.
crate::system_test_cases!({
    args: &["prog.o"],
    expected_stdout: "yacc prog.y\ncompile prog.c\nrm prog.c\n",
    expected_stderr: "",
    expected_files: &[],
    pre_hook: {
        std::fs::write("tests/scenarios/gnu_features/t39_intermediate/prog.y", "").unwrap();
    },
    post_hook: {
        let dir = "tests/scenarios/gnu_features/t39_intermediate";
        for f in ["prog.y", "prog.c", "prog.o"] {
            let _ = std::fs::remove_file(format!("{dir}/{f}"));
        }
    },
});
