//! # omake (Oxidized Make)
//!
//! This is an implementation of `make`, written in Rust. The goal is to provide an implementation
//! of `make` that can be used to process both BSD and GNU makefiles.

mod args;
mod context;
mod error;
mod expand;
mod makefile;
mod rule_map;
mod vars;

use std::env::{current_dir, set_current_dir};
use std::path::{Path, PathBuf};

use clap::Parser;

use args::Args;
use context::Context;
use error::{log_error, log_info};
use makefile::Makefile;

/// An ordered list of files which ought to be used to search for a makefile. POSIX specifies that
/// `makefile` must be checked before `Makefile`, so we also extend that to the BSD/GNU flavors.
const MAKEFILE_SEARCH: [&str; 6] = [
    "makefile",
    "Makefile",
    "BSDmakefile",
    "BSDMakefile",
    "GNUmakefile",
    "GNUMakefile",
];

const LICENSE: &str = include_str!("../LICENSE");

/// Search for a makefile to execute.
fn find_makefile() -> Option<PathBuf> {
    for file in MAKEFILE_SEARCH {
        if Path::new(file).is_file() {
            return Some(PathBuf::from(file));
        }
    }

    None
}

/// Helper to print an error message and exit with code 2.
fn exit_with(msg: impl AsRef<str>, context: Option<&Context>) -> ! {
    log_error(msg, context);
    std::process::exit(2)
}

fn main() {
    let args = Args::parse();

    // Print LICENSE, if requested.
    if args.license {
        println!("{}", LICENSE);
        return;
    }

    // Change to another directory, if specified by the arguments.
    let original_dir = if args.directory.is_empty() {
        None
    } else {
        // Remember the current directory to return to.
        let cwd = current_dir()
            .unwrap_or_else(|e| exit_with(format!("Failed to get cwd ({}).", e), None));

        // let dir = PathBuf::new();
        let dir = args
            .directory
            .iter()
            .fold(PathBuf::new(), |dir, d| dir.join(d));
        // for d in &args.directory {
        //     dir = dir.join(d);
        // }
        log_info(format!("Chdir to `{}`.", dir.display()), None);
        set_current_dir(&dir).unwrap_or_else(|e| exit_with(format!("Chdir failed: {}.", e), None));

        Some(cwd)
    };

    // Determine the makefile to read.
    let makefile_fn = match args.file {
        Some(ref file) => PathBuf::from(file),
        None => find_makefile().unwrap_or_else(|| exit_with("No makefile found.", None)),
    };

    // Parse the makefile.
    let makefile = match Makefile::new(makefile_fn, args) {
        Err(e) => exit_with(e.msg, Some(&e.context)),
        Ok(m) => m,
    };
    if let Err(e) = makefile.execute() {
        exit_with(e.msg, Some(&e.context));
    }

    // Go back to the original directory, if we chdir'd.
    if let Some(cwd) = original_dir {
        log_info(format!("Chdir back to `{}`.", cwd.display()), None);
        set_current_dir(&cwd).unwrap_or_else(|e| exit_with(format!("Chdir failed: {}.", e), None));
    }
}
