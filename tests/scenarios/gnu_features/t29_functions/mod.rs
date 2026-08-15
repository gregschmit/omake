// GNU text functions composed as in a real makefile: `patsubst`/`filter`/`sort`/`dir`/`notdir`/
// `words`/`firstword`/`addprefix` derive object files, include dirs, and flags from a source list.
// Cross-verified against gmake.
crate::system_test_cases!({
    args: &["all"],
    expected_stdout: "objs=[src/main.o src/util.o]\ndirs=[inc/ src/]\nhdrs=[util.h]\ncount=[3] first=[src/main.c]\nflags=[-Iinc/ -Isrc/]\n",
    expected_stderr: "",
    expected_files: &[],
});
