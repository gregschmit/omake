//! # omake (Oxidized Make)
//!
//! This is an implementation of `make`, written in Rust.

use std::path::{Path, PathBuf};

use clap::Parser;
use const_format::formatcp;

/// Only interface via the `omake` library (lib.rs).
use omake::{log_err, Context, Makefile};

const MAKEFILE_SEARCH: [&str; 6] = [
    "Makefile",
    "makefile",
    "BSDMakefile",
    "BSDmakefile",
    "GNUMakefile",
    "GNUmakefile",
];
const LICENSE: &str = include_str!("../LICENSE");

#[derive(Clone, Debug, Parser)]
#[clap(
    name = "make (oxidized)",
    version,
    about,
    after_help = formatcp!(
        "License:  {}\nSource:   {}", env!("CARGO_PKG_LICENSE"), env!("CARGO_PKG_REPOSITORY")
    ),
)]
struct Args {
    #[clap(short, long, visible_alias("makefile"))]
    /// Read FILE as the makefile.
    file: Option<String>,
    #[clap(short, long, value_name = "FILE", visible_alias("assume-old"))]
    /// Consider FILE to be very old and do not remake it.
    old_file: Vec<String>,
    #[clap(long)]
    /// Show full software license.
    license: bool,
}

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
fn exit_with<S: Into<String>>(msg: S, context: Option<&Context>) -> ! {
    log_err(msg, context);
    std::process::exit(2)
}

fn main() {
    let args = Args::parse();

    // Print LICENSE, is requested.
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
    let makefile = match Makefile::new(makefile_fn) {
        Err(e) => exit_with(e.msg, Some(&e.context)),
        Ok(m) => m,
    };
    dbg!(makefile);
}
