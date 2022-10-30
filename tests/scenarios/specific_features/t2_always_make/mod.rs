const OUTPUT: &str = "echo test > a
echo test > test\n";

crate::system_test_cases!([&["-B", "test"], OUTPUT, &[("test", "test\n")],],);
