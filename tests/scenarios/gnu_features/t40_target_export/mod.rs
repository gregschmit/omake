// A target-specific `export APPVAR = from-app` puts the variable in the recipe environment of the
// target AND its prerequisites (`helper` sees `$APPVAR`), but not of an unrelated goal (`plain`
// sees it empty) — matching GNU make's scoped export.
crate::system_test_cases!({
    args: &["app", "plain"],
    expected_stdout: "helper env APPVAR=from-app\napp sees APPVAR=from-app\nplain env APPVAR=\n",
    expected_stderr: "",
    expected_files: &[],
});
