crate::system_test_cases!([
    &[],
    "echo \"This is a test\" > test\n",
    &[("test", "This is a test\n")]
],);
