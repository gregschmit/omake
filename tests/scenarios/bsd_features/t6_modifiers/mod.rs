// BSD variable modifiers (`${VAR:M...}`, `:S/.../.../`, `:T`, `:H`, `:E`, `:R`, `:tu`, `:Q`, `:U`)
// transform a variable's value at expansion time, and chain left to right. A `${VAR:X=Y}` form is
// still a GNU substitution reference (the `=` disambiguates). Output matches bmake byte-for-byte.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "M: src/net.c src/io.c main.c\nN: lib/util.h\nS: src/net.o src/io.o lib/util.h main.o\nT: net.c io.c util.h main.c\nH: src src lib .\nE: c c h c\nR: src/net src/io lib/util main\nchain: net.o io.o main.o\ncase: NET.C IO.C MAIN.C\nQ: src/net.c\\ src/io.c\\ lib/util.h\\ main.c\nU: default\n",
    expected_stderr: "",
    expected_files: &[],
});
