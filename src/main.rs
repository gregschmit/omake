//! # omake (Oxidized Make)
//!
//! This is an implementation of `make`, written in Rust.

mod args;

use std::path::{Path, PathBuf};

use clap::Parser;

/// Only interface via the `omake` library (`lib/_lib.rs`).
use omake::{log_err, Context, Makefile, MAKEFILE_SEARCH};

use args::Args;

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
    log_err(msg, context);
    std::process::exit(2)
}

fn main() {
    let args = Args::parse();

    // Print LICENSE, if requested.
    if args.license {
        println!("{}", LICENSE);
        return;
    }

    // Determine the makefile to read.
    let makefile_fn = match args.file {
        Some(ref file) => PathBuf::from(file),
        None => find_makefile().unwrap_or_else(|| exit_with("No makefile found.", None)),
    };

    // Parse the makefile.
    let opts = args.to_opts();
    let makefile = match Makefile::new(makefile_fn, opts) {
        Err(e) => exit_with(e.msg, Some(&e.context)),
        Ok(m) => m,
    };
    if let Err(e) = makefile.execute(args.targets) {
        exit_with(e.msg, Some(&e.context));
    }
}
