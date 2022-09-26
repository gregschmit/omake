crate::system_test_cases!(
    [&[], "echo \"This is a test\"\nThis is a test\n", &[]],
    [&["test"], "echo \"This is a test\"\nThis is a test\n", &[]],
);
