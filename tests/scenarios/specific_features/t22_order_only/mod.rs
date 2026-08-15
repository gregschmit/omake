// Order-only prerequisites (after `|`) are built like normal ones but are excluded from `$^` and
// listed in `$|`; their mtime does not force the target to rebuild.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "made-src\nmade-oo\nnormal=src orderonly=oo\n",
    expected_stderr: "",
    expected_files: &[],
});
