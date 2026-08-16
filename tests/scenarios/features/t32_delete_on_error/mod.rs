// With `.DELETE_ON_ERROR:`, a recipe that creates its target and then fails has that (now partial)
// target deleted, so a later build does not treat the broken output as up to date. The build still
// exits 2. This matches gmake, which only deletes when the recipe actually modified the target.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "building\n",
    expected_stderr: "omake: INFO  | Deleting file 'artifact'\nomake: ERROR [Makefile:2] | Failed with code 1.\n  |\n2 | artifact:\n  | \n\n",
    expected_files: &[],
    expected_code: Some(2),
    post_hook: {
        let artifact = "tests/scenarios/features/t32_delete_on_error/artifact";
        assert!(
            !std::path::Path::new(artifact).exists(),
            "artifact should be deleted on error",
        );
        let _ = std::fs::remove_file(artifact);
    },
});
