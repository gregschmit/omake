const OUTPUT: &str = "echo \"test1\"
test1
echo test2
test2
echo \"test3\"
test3
echo \"test4\"
test4
echo \"test5\"
test5\n";

crate::system_test_cases!({
    args: &[], expected_stdout: OUTPUT, expected_stderr: "", expected_files: &[]
});
