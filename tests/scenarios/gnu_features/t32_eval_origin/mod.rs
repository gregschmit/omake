// `$(eval)` + `$(origin)`: the canonical generator loop `$(foreach ...,$(eval $(call template)))`
// defines per-item variables from a define/endef template, and `$(origin)` reports where variables
// came from (file / default / undefined / automatic). Cross-verified against gmake.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "built=[from-alpha] [from-beta]\norigins=[file] [default] [undefined] [automatic]\n",
    expected_stderr: "",
    expected_files: &[],
});
