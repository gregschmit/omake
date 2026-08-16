// An empty-but-present recipe (`built: ;`) still marks the target as "having commands", so an
// up-to-date target reports "'X' is up to date." rather than "Nothing to be done for 'X'." — the
// distinction GNU make draws between a present-but-empty recipe and no recipe at all (the common
// `config.status: ;` idiom).
crate::system_test_cases!({
    args: &["built"],
    expected_stdout: "",
    expected_stderr: "make: INFO  | 'built' is up to date.\n",
    expected_files: &[],
    pre_hook: {
        std::fs::write("tests/scenarios/features/t33_empty_recipe/built", "").unwrap();
    },
    post_hook: {
        let _ = std::fs::remove_file("tests/scenarios/features/t33_empty_recipe/built");
    },
});
