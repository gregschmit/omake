//! This module provides the `clap`-based `Args` struct. This is also used for invocations of
//! sub-make using `$(MAKE)`.

use clap::Parser;
use const_format::formatcp;

const SUBMAKE_FORBIDDEN_FLAGS: [&str; 5] = ["-j", "-C", "-f", "-o", "-W"];

const BUILD_MODE: &str = if cfg!(debug_assertions) {
    "Debug"
} else {
    "Release"
};

/// Represents the `clap`-based arguments provided by this binary.
#[derive(Clone, Debug, Parser)]
#[clap(
name = "make (oxidized)",
version,
about,
after_help = formatcp ! (
"License:     {}\nSource:      {}\nVersion:     {}\nBuild type:  {}",
env ! ("CARGO_PKG_LICENSE"),
env ! ("CARGO_PKG_REPOSITORY"),
env ! ("CARGO_PKG_VERSION"),
BUILD_MODE
),
)]
pub struct Args {
    /// Target(s) (if none specifired, use first regular target).
    #[arg()]
    pub targets: Vec<String>,

    /// Read FILE as the makefile.
    #[arg(short, long, visible_alias("makefile"))]
    pub file: Option<String>,

    /// Ignored for compatibility.
    #[arg(short = 'b')]
    pub b: bool,

    /// Ignored for compatibility.
    #[arg(short = 'm')]
    pub m: Option<Option<String>>,

    /// Unconditionally make all targets.
    #[arg(short = 'B', long)]
    pub always_make: bool,

    /// Change to DIR before doing anything.
    #[arg(short = 'C', long, value_name = "DIR")]
    pub directory: Vec<String>,

    /// Ignore errors from recipes.
    #[arg(short, long)]
    pub ignore_errors: bool,

    /// Don't execute recipes; just print them.
    #[arg(
    short = 'n',
    long = "just-print",
    visible_alias("dry-run"),
    visible_alias("recon")
    )]
    pub just_print: bool,

    /// Consider FILE to be very old and do not remake it.
    #[arg(short, long, value_name = "FILE", visible_alias("assume-old"))]
    pub old_file: Vec<String>,

    /// Consider FILE to be very new to simulate "what if" it changed.
    #[arg(
    short = 'W',
    long = "what-if",
    value_name = "FILE",
    visible_alias("new-file"),
    visible_alias("assume-new")
    )]
    pub new_file: Vec<String>,

    /// Print software license.
    #[arg(long)]
    pub license: bool,
}

/// Converts the arguments to a string that can be passed to a sub-make invocation.
pub(crate) fn args_to_submake_str() -> String {
    let mut single_flags = Vec::new();
    let mut arguments = Vec::new();
    // Rudimetary MAKEFLAGS parsing, the '-j' flag handling is not implemented yet.
    // TODO: This should probably be a `Result` instead of a `panic!`.
    // NOTE: Maybe change the way this is done to a pure IPC solution? when sub-make is used?
    std::env::args()
        .collect::<Vec<_>>()
        .iter()
        .for_each(|arg| {
            let mut arg_mod = arg.clone();
            // If the argument contains a space, we need to quote it.
            if arg.contains(' ') {
                arg_mod = format!("\"{}\"", arg);
            }

            if SUBMAKE_FORBIDDEN_FLAGS.contains(&arg_mod.as_str()) {
                return;
            }

            if arg_mod.starts_with("--") {
                arguments.push(arg[2..].to_string());
            } else if arg_mod.starts_with('-') {
                single_flags.push(arg[1..].to_string());
            }
        });

    let mut result = "-".to_string();
    if single_flags.is_empty() {
        result = "".to_string();
    } else {
        result.insert_str(1, &single_flags.join(""));
    }

    for argument in arguments {
        result.push_str(&format!(" --{}", argument));
    }

    result
}
