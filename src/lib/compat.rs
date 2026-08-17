//! `--compat`: the make-emulation target. Currently it selects the diagnostic-output format so
//! omake can match GNU make or bmake byte-for-byte (for scripts/tools that parse a specific make's
//! output, and to measure real conformance rather than format noise). omake keeps its own clean
//! output by default (`Native`). Behavioral emulation of the genuine GNU↔BSD divergences may extend
//! this later.

/// The make whose diagnostic output omake emulates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Compat {
    /// omake's native output (`ERROR [file:N] | … ` with a source caret).
    #[default]
    Native,
    /// GNU make (`<prog>: *** [file:N: target] Error C`, `… .  Stop.`).
    Gnu,
    /// NetBSD bmake (`*** Error code C`, `<prog>: stopped in <dir>`).
    Bsd,
}

impl Compat {
    /// The line for a recipe that failed with `reason` (already the make-style reason, e.g.
    /// `Error 1` or a signal name like `Terminated`), or `None` in native mode (where the caller
    /// uses omake's caret rendering). `prefix` is the invocation name (`omake`, `omake[1]`), `file`
    /// is the makefile path.
    pub fn recipe_error(
        self,
        prefix: &str,
        file: &str,
        line: usize,
        target: &str,
        reason: &str,
    ) -> Option<String> {
        match self {
            Compat::Native => None,
            Compat::Gnu => Some(format!("{prefix}: *** [{file}:{line}: {target}] {reason}")),
            // bmake prints "*** Error code C" (or the signal) then "<prog>: stopped in <dir>". We
            // approximate its single error line; the trailing "stopped in" location is omitted.
            Compat::Bsd => Some(format!("*** {reason}")),
        }
    }

    /// The line for a fatal error that is not a recipe failure or a missing rule (e.g. `$(error)`,
    /// a parse error, or a setup error). A makefile-content error carries a `file`/`line`; a setup
    /// error (no makefile, etc.) does not and uses the program prefix. `None` in native mode.
    pub fn fatal_error(
        self,
        prefix: &str,
        file: Option<&str>,
        line: Option<usize>,
        msg: &str,
    ) -> Option<String> {
        // gmake/bmake append their own "Stop"/period, so drop a trailing one from omake's message.
        let text = msg.trim_end().trim_end_matches('.');
        match self {
            Compat::Native => None,
            // gmake: `file:line: *** msg.  Stop.` for content, `<prog>: *** msg.  Stop.` otherwise.
            Compat::Gnu => {
                let loc = match (file, line) {
                    (Some(f), Some(l)) => format!("{f}:{l}"),
                    _ => prefix.to_string(),
                };
                Some(format!("{loc}: *** {text}.  Stop."))
            }
            // bmake: `<prog>: "file" line N: msg` for content, `<prog>: msg` otherwise.
            Compat::Bsd => Some(match (file, line) {
                (Some(f), Some(l)) => format!("{prefix}: \"{f}\" line {l}: {text}"),
                _ => format!("{prefix}: {text}"),
            }),
        }
    }

    /// The line for a target that has no rule and no file. `needed_by` is the target that required
    /// it (empty for a top-level goal). `None` in native mode.
    pub fn no_rule_error(self, prefix: &str, target: &str, needed_by: &str) -> Option<String> {
        match self {
            Compat::Native => None,
            Compat::Gnu => {
                let by = if needed_by.is_empty() {
                    String::new()
                } else {
                    format!(", needed by '{needed_by}'")
                };
                Some(format!(
                    "{prefix}: *** No rule to make target '{target}'{by}.  Stop."
                ))
            }
            Compat::Bsd => Some(format!("{prefix}: don't know how to make {target}. Stop")),
        }
    }
}
