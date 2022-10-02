mod scenarios;

use std::collections::HashMap;
use std::fs;
use std::process::Command;

/// Represents a path to a test directory, arguments to pass to `omake`, and the expectations.
struct SystemTestCase {
    /// Path to this system test directory (relative to project root).
    pub path: String,
    /// Arguments to pass to `omake`.
    pub args: Vec<String>,
    /// Expected standard output.
    pub expected_stdout: String,
    /// Expected files that should be created, mapped to their content.
    pub expected_files: HashMap<String, String>,
}

impl SystemTestCase {
    /// Principal interface for constructing a system test, running it, and cleaning up. Note that
    /// cleanup is done by implementing the `Drop` trait. Since the system test is dropped at the
    /// end of this function's scope, it is cleaned up.
    pub fn execute(
        path: &str,
        args: &[&str],
        expected_stdout: &str,
        expected_files: &[(&str, &str)],
    ) {
        let system_test = Self {
            // Trim leading/trailing slashes in `path`.
            path: path.trim_matches('/').to_string(),
            args: Vec::from(args.iter().map(|a| a.to_string()).collect::<Vec<String>>()),
            expected_stdout: expected_stdout.into(),
            expected_files: HashMap::from(
                expected_files
                    .iter()
                    .map(|(f, c)| (f.to_string(), c.to_string()))
                    .collect::<HashMap<String, String>>(),
            ),
        };
        system_test.run();
    }

    /// Helper to run this system test and assert any expectations.
    fn run(&self) {
        // Determine path to `omake` (inside the `target/debug` directory). Note that we must
        // reference `omake` from within the directory that the system test is located.
        let omake_path = format!(
            "{}target/debug/omake",
            // Traverse back from test directory to project directory.
            "../".repeat(self.path.matches("/").count() + 1),
        );

        // Run `omake` inside the system test directory.
        let output = Command::new(omake_path)
            .args(&self.args)
            .current_dir(self.relative_path(&"".to_string()))
            .output()
            .unwrap();

        // Assert expected `stdout` (unless expected is blank).
        if !self.expected_stdout.is_empty() {
            assert_eq!(
                self.expected_stdout,
                String::from_utf8_lossy(&output.stdout)
            );
        }

        // Assert filesystem expectations.
        for (filename, expected_content) in &self.expected_files {
            let content = fs::read_to_string(self.relative_path(filename)).unwrap();
            assert_eq!(&content, expected_content);
        }
    }

    /// Helper to get the relative path to a file for this system test.
    fn relative_path(&self, file: &String) -> String {
        format!("{}/{file}", self.path)
    }
}

/// To handle filesystem cleanup, we implement the `Drop` trait. This covers both test success,
/// where the `SystemTestCase` goes out of scope and is therefore dropped, as well as test failure,
/// where the stack is unwinded and the `SystemTestCase` is therefore dropped.
impl Drop for SystemTestCase {
    fn drop(&mut self) {
        for (filename, _) in &self.expected_files {
            let _ = fs::remove_file(self.relative_path(filename));
        }
    }
}

/// Helper to define system test cases inside a system test module.
macro_rules! system_test_cases {
    ($([$args:expr, $expected_stdout:expr, $expected_files:expr $(,)?]),+ $(,)?) => {
        #[test]
        fn test() {
            // Get the path to the test file, pop off the filename, and convert to string.
            let mut path = std::path::PathBuf::from(file!());
            let _ = path.pop();
            let path = path.into_os_string().into_string().unwrap();

            // Run the specified test cases.
            $(
                crate::SystemTestCase::execute(&path, $args, $expected_stdout, $expected_files);
            )*
        }
    };
}
pub(crate) use system_test_cases;
