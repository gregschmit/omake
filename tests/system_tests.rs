use std::collections::HashMap;
use std::env::{current_dir, set_current_dir};
use std::fs;
use std::process::Command;

struct Expectations {
    pub files: HashMap<String, String>,
    pub stdout: String,
}

impl Expectations {
    pub fn new<const N: usize, S: Into<String>>(files: [(S, S); N], stdout: S) -> Self {
        Self {
            files: HashMap::from(files.map(|(f, c)| (f.into(), c.into()))),
            stdout: stdout.into(),
        }
    }
}

fn run_test<const N: usize>(name: &str, exp: Expectations, cleanup_files: [&str; N]) {
    // Remember current dir.
    let cwd = current_dir().unwrap().into_os_string();

    // Chdir into test dir.
    set_current_dir(format!("./tests/system_tests/{}", name)).unwrap();

    // Run make.
    let output = Command::new("omake").output().unwrap();

    // Assert expectations.
    // TODO: figure out way to see if test passed, maybe pass in expectations as arg?

    // Cleanup expected files.
    for file in cleanup_files {
        fs::remove_dir_all(file);
    }

    // Chdir back into original directory.
    set_current_dir(cwd).unwrap();
}

/// All system tests are in this single function. Since `std::env::set_current_dir` is not thread
/// safe (affects the whole process), these cannot be parallelized. In the future we might consider
/// breaking these apart to allow parallelization.
///
/// TODO: get this working.
/// #[test]
fn system_tests() {
    let exp = Expectations::new([], "This is a test\n");
    run_test("simple", exp, []);
}
