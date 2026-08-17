// The default `SUFFIXES` variable holds gmake's exact 35-entry suffix list. `.SUFFIXES` is a
// special *target*, not a variable: `$(.SUFFIXES)` is empty and `$(origin .SUFFIXES)` is
// `undefined`, while `$(origin SUFFIXES)` is `default`. Matches gmake.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "SUFFIXES=[.out .a .ln .o .c .cc .C .cpp .p .f .F .m .r .y .l .ym .yl .s .S .mod .sym .def .h .info .dvi .tex .texinfo .texi .txinfo .w .ch .web .sh .elc .el]\nDOTSUFFIXES=[]\norigin_SUFFIXES=[default]\norigin_DOTSUFFIXES=[undefined]\n",
    expected_stderr: "",
    expected_files: &[],
});
