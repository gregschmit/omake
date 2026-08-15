// Assignment operators and flavors:
// - `=` is lazy: B reflects A's latest value ("changed"), not the value when B was defined.
// - `:=`/`::=` are simple (expand now).
// - `?=` sets only if undefined, so C keeps "first" and CC keeps its default "cc".
// - `+=` appends (raw for recursive vars, expanded for simple ones).
// - `!=` stores the shell command's output.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "B=changed-deferred\nC=first\nCC=cc\nD=base more\nE=x y\nF=hello from shell\n",
    expected_stderr: "",
    expected_files: &[],
});
