//! Opt-in conformance regression gate.
//!
//! These tests run the external GNU make and NetBSD bmake suites (via `tests/conformance/run_gmake`
//! / `run_bmake`) and fail if the pass count drops below a recorded baseline. They are `#[ignore]`d
//! so the normal `cargo test` loop stays fast and does not require perl / bmake / network:
//!
//! ```sh
//! cargo test --test conformance_regression -- --ignored
//! ```
//!
//! Bump the baselines below whenever the pass count improves, so the new floor is enforced.

use std::process::Command;

/// GNU make suite pass floor (omake `--compat=gnu`, gmake 4.4.1). See `README.md`.
const GMAKE_BASELINE: u32 = 717;
/// NetBSD bmake unit-test pass floor (omake `--compat=bsd`, bmake 20230711).
const BMAKE_BASELINE: u32 = 91;

/// Run a conformance runner with the freshly built omake, returning its combined output.
fn run(script: &str) -> String {
    let output = Command::new(script)
        .env("OMAKE", env!("CARGO_BIN_EXE_omake"))
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn {script}: {e}"));
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

/// Parse the leading `<passed>/<total>` count from a runner's summary line (the first line whose
/// text mentions "passed" and carries an `X/Y` token).
fn passed_count(out: &str) -> u32 {
    for line in out.lines() {
        if !line.contains("passed") {
            continue;
        }
        for token in line.split_whitespace() {
            if let Some((passed, _)) = token.split_once('/')
                && !passed.is_empty()
                && passed.bytes().all(|b| b.is_ascii_digit())
            {
                return passed.parse().expect("digits parse as u32");
            }
        }
    }
    panic!("could not find a `<passed>/<total>` count in conformance output:\n{out}");
}

#[test]
#[ignore = "runs the external GNU make suite (needs perl, and network on first vendor); ~5 min"]
fn gmake_conformance_no_regression() {
    let out = run("tests/conformance/run_gmake");
    let passed = passed_count(&out);
    assert!(
        passed >= GMAKE_BASELINE,
        "GNU make conformance regressed: {passed} < {GMAKE_BASELINE} baseline.\n{out}"
    );
}

#[test]
#[ignore = "runs the external NetBSD bmake unit-tests (needs bmake installed); ~1 min"]
fn bmake_conformance_no_regression() {
    let out = run("tests/conformance/run_bmake");
    let passed = passed_count(&out);
    assert!(
        passed >= BMAKE_BASELINE,
        "BSD make conformance regressed: {passed} < {BMAKE_BASELINE} baseline.\n{out}"
    );
}
