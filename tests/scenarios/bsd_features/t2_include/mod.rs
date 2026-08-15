// BSD `.include "file"` pulls in another makefile; `.sinclude` (and `.-include`) tolerate a missing
// file. Cross-verified against bmake.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "INCVAL=from_include\n",
    expected_stderr: "",
    expected_files: &[],
});
