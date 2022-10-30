//! This module provides the `clap`-based `Args` struct and also a translation to `omake::Options`.
//!
//! The library portion of this software does not want to include `clap` as a dependency. To that
//! end, there is an `Options` struct where various options may be defined and then passed to the
//! `Makefile` constructor. We provide a facility `to_options` to translate `Args` to `Options`.

use clap::Parser;
use const_format::formatcp;

use omake::Options;

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
    // Start of `omake::Options` analogs.
    // These doc-comments should match the doc-comments in `omake::Options`.
    //
    //
    /// Unconditionally make all targets.
    #[arg(short = 'B', long = "always-make")]
    pub always_make: bool,

    /// Consider FILE to be very old and do not remake it.
    #[arg(short, long, value_name = "FILE", visible_alias("assume-old"))]
    pub old_file: Vec<String>,
}

impl Args {
    /// Helper to construct an `Options` instance from `self`.
    pub fn to_options(&self) -> Options {
        Options {
            always_make: self.always_make,
            old_file: self.old_file.clone(),
        }
    }
}
