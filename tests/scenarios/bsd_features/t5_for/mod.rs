// A BSD `.for VAR in LIST / .endfor` loop repeats its body once per item, substituting the loop
// variable (here into a recipe). The item list is variable-expanded first, matching bmake.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "build net\nbuild io\nbuild fs\n",
    expected_stderr: "",
    expected_files: &[],
});
