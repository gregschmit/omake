// An inline `;`-recipe containing a literal `=` (`all: ; @echo "A=1"`) is a rule with a recipe, not
// a target-specific variable assignment: the `=` follows the `;`, so it is part of the recipe. This
// guards against mistaking such a common recipe for `target: VAR = value`.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "A=1\n",
    expected_stderr: "",
    expected_files: &[],
});
