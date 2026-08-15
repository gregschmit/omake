mod scenarios;

use std::collections::HashMap;
use std::fs;
use std::process::Command;

/// A test directory, arguments to pass to `omake`, and the test expectations.
struct SystemTestCase {
    /// Path to this system test directory (relative to project root).
    pub path: String,
    /// Arguments to pass to `omake`.
    pub args: Vec<String>,
    /// Expected STDOUT.
    pub expected_stdout: String,
    /// Expected STDERR.
    pub expected_stderr: String,
    /// Expected files that should be created, mapped to their content.
    pub expected_files: HashMap<String, String>,
    /// Expected process exit code (`None` skips the check).
    pub expected_code: Option<i32>,
}

impl SystemTestCase {
    /// Construct a system test, run it, and clean up. Note that cleanup is done by implementing the
    /// `Drop` trait. Since the system test is dropped at the end of this function's scope, it is
    /// cleaned up.
    pub fn execute(
        path: &str,
        args: &[&str],
        expected_stdout: &str,
        expected_stderr: &str,
        expected_files: &[(&str, &str)],
        expected_code: Option<i32>,
    ) {
        let system_test = Self {
            // Trim leading/trailing slashes in `path`.
            path: path.trim_matches('/').to_string(),
            args: args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            expected_stdout: expected_stdout.into(),
            expected_stderr: expected_stderr.into(),
            expected_files: expected_files
                .iter()
                .map(|(f, c)| (f.to_string(), c.to_string()))
                .collect::<HashMap<_, _>>(),
            expected_code,
        };
        system_test.run();
    }

    /// Run this system test and assert any expectations.
    fn run(&self) {
        // `CARGO_BIN_EXE_omake` is set by Cargo for integration tests and guarantees the binary is
        // built first, so we don't have to hand-compute a `target/<profile>` path.
        let omake_path = env!("CARGO_BIN_EXE_omake");

        // Run `omake` inside the system test directory. Clear inherited make state so scenarios see
        // a clean environment regardless of how the test suite itself was invoked.
        let output = Command::new(omake_path)
            .args(&self.args)
            .current_dir(self.relative_path(&"".to_string()))
            .env_remove("MAKEFLAGS")
            .env_remove("MAKELEVEL")
            .output()
            .unwrap_or_else(|e| panic!("failed to run omake at {omake_path}: {e}"));

        // Assert expected STDOUT (unless expected is sentinel `"?"`).
        if self.expected_stdout != "?" {
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                self.expected_stdout,
                "STDOUT should match the expected STDOUT.",
            );
        }

        // Assert expected STDERR (unless expected is sentinel `"?"`).
        if self.expected_stderr != "?" {
            assert_eq!(
                String::from_utf8_lossy(&output.stderr),
                self.expected_stderr,
                "STDERR should match the expected STDERR.",
            );
        }

        // Assert the process exit code.
        if let Some(code) = self.expected_code {
            assert_eq!(
                output.status.code(),
                Some(code),
                "Exit code should match the expected code.",
            );
        }

        // Assert filesystem expectations.
        for (filename, expected_content) in &self.expected_files {
            let content =
                fs::read_to_string(self.relative_path(filename)).unwrap_or_else(|_| "".to_string());
            assert_eq!(
                &content, expected_content,
                "Content of {filename} should match expected content.",
            );
        }
    }

    /// Get the relative path to a file for this system test.
    fn relative_path(&self, file: &String) -> String {
        format!("{}/{file}", self.path)
    }
}

/// To handle filesystem cleanup, we implement the `Drop` trait. This covers both test success,
/// where the `SystemTestCase` goes out of scope and is therefore dropped, as well as test failure,
/// where the stack is unwinded and the `SystemTestCase` is therefore dropped.
impl Drop for SystemTestCase {
    fn drop(&mut self) {
        for filename in self.expected_files.keys() {
            let _ = fs::remove_file(self.relative_path(filename));
        }
    }
}

/// Define system test cases inside a system test module.
macro_rules! system_test_cases {
    (
        $({
            args: $args:expr,
            expected_stdout: $expected_stdout:expr,
            expected_stderr: $expected_stderr:expr,
            expected_files: $expected_files:expr
            $(, expected_code: $expected_code:expr)?
            $(, pre_hook: $pre_hook:block)?
            $(, post_hook: $post_hook:block)?
            $(,)?
        }),+ $(,)?
    ) => {
        #[test]
        fn test() {
            // Get the path to the test file, pop off the filename, and convert to string.
            let mut path = std::path::PathBuf::from(file!());
            let _ = path.pop();
            let path = path.into_os_string().into_string().unwrap();

            // Run the specified test cases.
            $(
                $($pre_hook)?
                {
                    // Default to asserting a successful (0) exit; a case may override it.
                    #[allow(unused_mut, unused_assignments)]
                    let mut expected_code: Option<i32> = Some(0);
                    $( expected_code = $expected_code; )?
                    crate::SystemTestCase::execute(
                        &path, $args, $expected_stdout, $expected_stderr, $expected_files,
                        expected_code,
                    );
                }
                $($post_hook)?
            )*
        }
    };
}
pub(crate) use system_test_cases;
