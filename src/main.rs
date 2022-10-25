//! # omake (Oxidized Make)
//!
//! This is an implementation of `make`, written in Rust.

use std::path::{Path, PathBuf};

use clap::Parser;
use const_format::formatcp;

/// Only interface via the `omake` library (`lib.rs`).
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
    /// Target(s) (if none specifired, use first regular target).
    #[arg()]
    targets: Vec<String>,

    /// Ignored for compatibility.
    #[arg(short = 'b')]
    b: bool,
    /// Ignored for compatibility.
    #[arg(short = 'm')]
    m: Option<Option<String>>,

    /// Unconditionally make all targets.
    #[arg(short = 'B', long = "always-make")]
    always_make: bool,

    /// Read FILE as the makefile.
    #[arg(short, long, visible_alias("makefile"))]
    file: Option<String>,
    /// Consider FILE to be very old and do not remake it.
    #[arg(short, long, value_name = "FILE", visible_alias("assume-old"))]
    old_file: Vec<String>,

    /// Show full software license.
    #[arg(long)]
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
    let makefile = match Makefile::new(makefile_fn) {
        Err(e) => exit_with(e.msg, Some(&e.context)),
        Ok(m) => m,
    };
    if let Err(e) = makefile.execute(args.targets) {
        exit_with(e.msg, Some(&e.context));
    }
}
