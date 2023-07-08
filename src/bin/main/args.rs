//! This module provides the `clap`-based `Args` struct. This is also used for invocations of
//! sub-make using `$(MAKE)`.

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

impl From<Args> for Opts {
    fn from(args: Args) -> Self {
        Self {
            always_make: args.always_make,
            ignore_errors: args.ignore_errors,
            just_print: args.just_print,
            old_file: args.old_file,
            new_file: args.new_file,
        }
    }
}
