//! This module provides the `clap`-based `Args` struct. This is also used for invocations of
//! sub-make using `$(MAKE)`.

use clap::Parser;
use const_format::formatcp;

use omake::Opts;

/// The `clap`-based arguments provided by this binary.
#[derive(Clone, Debug, Parser)]
#[clap(
    name = "omake",
    version,
    about,
    after_help = formatcp!(
        "License:  {}\nSource:   {}", env!("CARGO_PKG_LICENSE"), env!("CARGO_PKG_REPOSITORY")
    ),
)]
pub struct Args {
    /// Target(s) (if none specified, use first regular target).
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

    /// Continue as much as possible after an error.
    #[arg(short, long)]
    pub keep_going: bool,

    /// Don't echo recipes as they are run.
    #[arg(short, long, visible_alias("quiet"))]
    pub silent: bool,

    /// Disable the built-in implicit rules.
    #[arg(short = 'r', long = "no-builtin-rules")]
    pub no_builtin_rules: bool,

    /// Disable the built-in variable settings (implies -r).
    #[arg(short = 'R', long = "no-builtin-variables")]
    pub no_builtin_variables: bool,

    /// Environment variables override makefiles.
    #[arg(short = 'e', long = "environment-overrides")]
    pub environment_overrides: bool,

    /// Touch targets instead of remaking them.
    #[arg(short = 't', long)]
    pub touch: bool,

    /// Search DIR for included makefiles.
    #[arg(short = 'I', long = "include-dir", value_name = "DIR")]
    pub include_dir: Vec<String>,

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

    /// Print the working directory before and after other processing (the default with `-C`).
    #[arg(short = 'w', long = "print-directory")]
    pub print_directory: bool,

    /// Turn off `-w`, even if it was turned on implicitly.
    #[arg(long = "no-print-directory")]
    pub no_print_directory: bool,

    /// Print software license.
    #[arg(long)]
    pub license: bool,
}

/// Fold a `-I` directory list left to right, honoring `-I -` (which clears the list accumulated so
/// far, matching gmake).
fn fold_include_dirs(entries: &[String]) -> Vec<String> {
    let mut dirs = Vec::new();
    for e in entries {
        if e == "-" {
            dirs.clear();
        } else {
            dirs.push(e.clone());
        }
    }
    dirs
}

impl From<Args> for Opts {
    fn from(args: Args) -> Self {
        Self {
            always_make: args.always_make,
            ignore_errors: args.ignore_errors,
            just_print: args.just_print,
            keep_going: args.keep_going,
            silent: args.silent,
            // `-R` (no built-in variables) implies `-r` (no built-in rules).
            no_builtin_rules: args.no_builtin_rules || args.no_builtin_variables,
            no_builtin_variables: args.no_builtin_variables,
            environment_overrides: args.environment_overrides,
            touch: args.touch,
            include_dirs: fold_include_dirs(&args.include_dir),
            old_file: args.old_file,
            new_file: args.new_file,
        }
    }
}
