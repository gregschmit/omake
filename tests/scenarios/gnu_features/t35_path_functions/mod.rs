// `$(realpath)` canonicalizes an existing path (dropping a nonexistent one) and `$(abspath)`
// resolves `..` lexically. `$(notdir ...)` keeps the assertion machine-independent. Verified vs
// gmake.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "rp=[foo.txt] miss=[] abs=[foo.txt]\n",
    expected_stderr: "",
    expected_files: &[],
});
