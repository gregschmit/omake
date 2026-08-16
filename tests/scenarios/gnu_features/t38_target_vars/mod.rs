// A target-specific variable (`app: FLAGS = app-specific`) is scoped to the target and its
// prerequisites: building `app` and its prereq `helper` both see the app-specific value, while an
// unrelated goal `other` sees the global one. This is GNU make's dynamically-scoped
// target-specific variable behavior.
crate::system_test_cases!({
    args: &["app", "other"],
    expected_stdout: "helper FLAGS=app-specific\napp FLAGS=app-specific\nother FLAGS=global\n",
    expected_stderr: "",
    expected_files: &[],
});
