// BSD dot-conditionals: `.if`/`.ifdef`/`.ifndef`/`.elif`/`.else`/`.endif` with numeric, string,
// and relational comparisons (`==`/`!=`/`>=`, hex literals), `defined()`, `!` negation, bare
// truthiness, and whitespace-indented nesting (`.  ifdef`). Cross-verified against bmake.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "A=num_eq B=num_ne C=negated D=truthy E=zero_false F=nested G=elif_branch H=num I=str J=rel K=hex\n",
    expected_stderr: "",
    expected_files: &[],
});
