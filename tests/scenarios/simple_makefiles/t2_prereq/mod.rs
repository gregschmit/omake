crate::system_test_cases!(
    [
        &[],
        "echo \"This is a test\" > test\n",
        &[("test", "This is a test\n")],
    ],
    [
        &["test_prereq"],
        "",
        &[("test", "This is a test\n"), ("test_prereq", "test2\n")],
    ],
);
