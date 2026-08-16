// Several single-colon rules for one target are merged: `prog: main.o` and `prog: util.o` combine
// into one target whose prerequisites are the recipe-bearing stanza's first (`util.o`), then the
// others in definition order (`main.o`) — so `$^` is `util.o main.o`, matching GNU make.
crate::system_test_cases!({
    args: &[],
    expected_stdout: "cc util.o\ncc main.o\nlink: util.o main.o\n",
    expected_stderr: "",
    expected_files: &[],
});
