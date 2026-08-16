// A `private` target-specific variable is scoped to the target's own recipe: the prerequisite
// `dep` does not inherit `SECRET`, but a normal target variable (`SHARED`) still propagates to it.
// Matches GNU make.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "dep: SECRET=[] SHARED=[overridden]\napp: SECRET=[hidden] SHARED=[overridden]\n",
    expected_stderr: "",
    expected_files: &[],
});
