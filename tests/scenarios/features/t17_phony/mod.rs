// A `.PHONY` target runs its recipe even when a file of the same name exists (the pre_hook creates
// a file named `clean`, which without .PHONY would make the target look up to date). A target
// declared only via `.PHONY` (`noop`, with no rule of its own) is a valid no-op: like gmake, it
// reports "Nothing to be done".
crate::system_test_cases!(
    {
        args: &["clean"],
        expected_stdout: "cleaning\n",
        expected_stderr: "",
        expected_files: &[],
        pre_hook: {
            std::fs::write("tests/scenarios/features/t17_phony/clean", "").unwrap();
        },
        post_hook: {
            let _ = std::fs::remove_file("tests/scenarios/features/t17_phony/clean");
        },
    },
    {
        args: &["noop"],
        expected_stdout: "omake: Nothing to be done for 'noop'.\n",
        expected_stderr: "",
        expected_files: &[],
        expected_code: Some(0),
    },
);
