//! This module provides the `clap`-based `Args` struct. This is also used for invocations of
//! sub-make using `$(MAKE)`.

use clap::{Parser, ValueEnum};
use const_format::formatcp;

use omake::{Compat, Opts};

/// `--compat` targets exposed on the command line (native omake is the default when the flag is
/// absent, so it is not a selectable value here).
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CompatArg {
    Gnu,
    Bsd,
}

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
    pub file: Vec<String>,

    /// Ignored for compatibility.
    #[arg(short = 'b')]
    pub b: bool,

    /// Ignored for compatibility.
    #[arg(short = 'm')]
    pub m: Option<Option<String>>,

    /// Print the data base (accepted; not yet implemented).
    #[arg(short = 'p', long = "print-data-base")]
    pub print_data_base: bool,

    /// Print lots of debugging information (accepted; not yet implemented).
    #[arg(short = 'd', long = "debug", value_name = "FLAGS", num_args = 0..=1, default_missing_value = "a")]
    pub debug: Option<String>,

    /// Print tracing information (accepted; not yet implemented).
    #[arg(long)]
    pub trace: bool,

    /// Warn when an undefined variable is referenced (accepted; not yet implemented).
    #[arg(long = "warn-undefined-variables")]
    pub warn_undefined_variables: bool,

    /// Don't start multiple jobs unless load is below N (accepted; omake is serial).
    #[arg(short = 'l', long = "load-average", visible_alias("max-load"), value_name = "N", num_args = 0..=1)]
    pub load_average: Option<f64>,

    /// Synchronize output of parallel jobs (accepted; omake is serial).
    #[arg(short = 'O', long = "output-sync", value_name = "TYPE", num_args = 0..=1, default_missing_value = "target")]
    pub output_sync: Option<String>,

    /// Print the version number (alias for --version).
    #[arg(short = 'v', long = "show-version")]
    pub show_version: bool,

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
    #[arg(short, long, overrides_with = "no_keep_going")]
    pub keep_going: bool,

    /// Turn off -k (the default; cancels a `-k` inherited via MAKEFLAGS).
    #[arg(short = 'S', long = "no-keep-going", overrides_with = "keep_going")]
    pub no_keep_going: bool,

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

    /// Run no recipe; exit 0 if targets are up to date, 1 otherwise.
    #[arg(short = 'q', long)]
    pub question: bool,

    /// Allow N jobs at once (accepted; omake currently builds serially).
    #[arg(short = 'j', long = "jobs", value_name = "N", num_args = 0..=1, default_missing_value = "0")]
    pub jobs: Option<Option<u32>>,

    /// Emulate another make's diagnostic output.
    #[arg(long, value_name = "MAKE")]
    pub compat: Option<CompatArg>,

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
            no_keep_going: args.no_keep_going,
            silent: args.silent,
            // `-R` (no built-in variables) implies `-r` (no built-in rules).
            no_builtin_rules: args.no_builtin_rules || args.no_builtin_variables,
            no_builtin_variables: args.no_builtin_variables,
            environment_overrides: args.environment_overrides,
            touch: args.touch,
            include_dirs: fold_include_dirs(&args.include_dir),
            question: args.question,
            // `-j` with no number (or `-j0`) means unlimited (0); `-jN` means N.
            jobs: args.jobs.map(|inner| inner.unwrap_or(0) as usize),
            compat: match args.compat {
                Some(CompatArg::Gnu) => Compat::Gnu,
                Some(CompatArg::Bsd) => Compat::Bsd,
                None => Compat::Native,
            },
            old_file: args.old_file,
            new_file: args.new_file,
        }
    }
}
