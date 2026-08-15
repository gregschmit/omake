// A circular dependency (a -> b -> a) is detected and dropped with a warning instead of recursing
// forever, then the build continues and exits successfully (matching GNU make).
crate::system_test_cases!({
    args: &["a"],
    expected_stdout: "make-b\nmake-a\n",
    expected_stderr: "make: WARN  | Circular b <- a dependency dropped.\n",
    expected_files: &[],
    expected_code: Some(0),
});
