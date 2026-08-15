// Automatic variables: `$@` (target), `$<` (first prereq), `$^` (prereqs deduped), `$+` (prereqs
// with duplicates), `$?` (prereqs newer than target — all here, deduped, since `all` is absent),
// and the `$(@D)`/`$(@F)` directory/file parts.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "target=all\nfirst=dep1\ndedup=dep1 dep2\nwithdup=dep1 dep2 dep2\nqmark=dep1 dep2\ndf=./all\n",
    expected_stderr: "",
    expected_files: &[],
});
