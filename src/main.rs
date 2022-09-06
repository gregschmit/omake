//! # Make (oxidized)
//!
//! This is an implementation of `make`, written in Rust.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use clap::Parser;
use const_format::formatcp;

/// Only interface via the `make` library (libmake.rs).
use omake::Makefile;

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
fn find_makefile() -> Option<String> {
    for file in MAKEFILE_SEARCH {
        if Path::new(file).is_file() {
            return Some(file.to_string());
        }
    }

    None
}

/// Helper to print an error message and exit with code 2.
fn exit_with<S: Into<String>>(msg: S) -> ! {
    println!("make: {}", msg.into());
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
        Some(ref file) => file.clone(),
        None => find_makefile().unwrap_or_else(|| exit_with("No makefile found.")),
    };

    // Parse the makefile.
    let file = File::open(makefile_fn)
        .unwrap_or_else(|e| exit_with(format!("Error reading makefile ({}).", e)));
    let stream = BufReader::new(file);
    let makefile = match Makefile::new(stream) {
        Err(e) => exit_with(e.to_string()),
        Ok(m) => m,
    };
    dbg!(makefile);
}
