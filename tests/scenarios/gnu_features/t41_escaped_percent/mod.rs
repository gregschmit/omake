// A backslash quotes a `%` in a substitution reference or pattern function (`$(patsubst)`,
// `$(filter)`): `\%` is a literal percent, not the wildcard. The first *unescaped* `%` is still the
// wildcard, so `x\%%` matches a literal `x%` followed by any stem. Matches GNU make (bmake does not
// unescape `\%`).
crate::system_test_cases!({
    args: &[],
    expected_stdout: "subref: HIT b%y cx\npatsubst: HIT b%y cx\nfilter: a%x\nwildcard: S-a S-b y\n",
    expected_stderr: "",
    expected_files: &[],
});
