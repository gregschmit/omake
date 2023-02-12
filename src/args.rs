//! This module provides the `clap`-based `Args` struct and also a translation to `omake::Opts`.
//!
//! The library portion of this software does not want to include `clap` as a dependency. To that
//! end, there is an `Opts` struct where various options may be defined and then passed to the
//! `Makefile` constructor. We provide a facility `to_opts` to translate `Args` to `Opts`.

use clap::Parser;
use const_format::formatcp;

use omake::Opts;

/// Represents the `clap`-based arguments provided by this binary.
#[derive(Clone, Debug, Parser)]
#[clap(
    name = "make (oxidized)",
    version,
    about,
    after_help = formatcp!(
        "License:  {}\nSource:   {}", env!("CARGO_PKG_LICENSE"), env!("CARGO_PKG_REPOSITORY")
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

    /// Print software license.
    #[arg(long, display_order = 9999)]
    pub license: bool,

    //
    // Start of `omake::Opts` analogs. All of these fields should exist in `omake::Opts`.
    //
    /// Unconditionally make all targets.
    #[arg(short = 'B', long = "always-make")]
    pub always_make: bool,

    /// Consider FILE to be very old and do not remake it.
    #[arg(short, long, value_name = "FILE", visible_alias("assume-old"))]
    pub old_file: Vec<String>,

    /// Consider FILE to be very new.
    #[arg(
        short = 'W',
        long = "what-if",
        value_name = "FILE",
        visible_alias("new-file"),
        visible_alias("assume-new")
    )]
    pub new_file: Vec<String>,
}

impl Args {
    /// Helper to construct an `Opts` instance from `self`.
    pub fn to_opts(&self) -> Opts {
        Opts {
            always_make: self.always_make,
            old_files: self.old_file.clone(),
            new_files: self.new_file.clone(),
        }
    }
}
