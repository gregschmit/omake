use std::collections::HashMap;
use std::fs;
use std::process::Command;

const SYSTEM_TESTS_DIR: &str = "./tests/system_tests";

/// Wrapper for the test name (path) and properties expected of the test.
struct SystemTest {
    pub name: String,
    pub created_files: HashMap<String, String>,
    pub stdout: String,
}

impl SystemTest {
    /// Construct a `SystemTest`, run it, and then return.
    pub fn execute<const N: usize>(name: &str, created_files: [(&str, &str); N], stdout: &str) {
        let system_test = Self {
            name: name.into(),
            created_files: HashMap::from(created_files.map(|(f, c)| (f.into(), c.into()))),
            stdout: stdout.into(),
        };

        system_test.run();
    }

    /// Run the subprocess and assert any expectations.
    fn run(&self) {
        // Run `omake` inside the system test directory. Note that the program path must be relative
        // to the `current_dir` of the spawned process.
        let output = Command::new("../../../target/debug/omake")
            .current_dir(self.relative_path(&"".to_string()))
            .output()
            .unwrap();

        // Assert expected `stdout`.
        assert_eq!(self.stdout, String::from_utf8_lossy(&output.stdout));

        // Assert `created_files` expectations.
        for (filename, expected_content) in &self.created_files {
            let content = fs::read_to_string(self.relative_path(filename)).unwrap();
            assert_eq!(&content, expected_content);
        }
    }

    /// Helper to get the relative path to a file for this system test.
    fn relative_path(&self, path: &String) -> String {
        format!("{}/{}/{}", SYSTEM_TESTS_DIR, self.name, path)
    }
}

/// Ensure created files for system tests are cleaned up by overloading `Drop`. This works because
/// the test harness ensures `panic = "unwind"` behavior.
impl Drop for SystemTest {
    fn drop(&mut self) {
        for (filename, _) in &self.created_files {
            let _ = fs::remove_file(self.relative_path(filename));
        }
    }
}

/// Principal interface for executing system tests.
#[test]
fn system_tests() {
    SystemTest::execute("simple", [], "echo \"This is a test\"\nThis is a test\n");
    SystemTest::execute(
        "simple_dependency",
        [("test", "This is a test\n")],
        "echo \"This is a test\" > test\n",
    );
}
