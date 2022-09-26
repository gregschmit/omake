crate::system_test_cases!(
    [
        &[],
        "echo \"This is a test\" > test\n",
        &[("test", "This is a test\n")],
    ],
    [&["dep2"], "", &[("dep2", "test2\n")],],
);
