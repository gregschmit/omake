//! # omake (Oxidized Make)
//!
//! This is an implementation of `make`, written in Rust. The goal is to provide an implementation
//! of `make` that can be used to process both BSD and GNU makefiles.

mod args;

use std::env;
use std::fs;
use std::path::PathBuf;

use clap::Parser;

use args::Args;

use omake::{Context, DefaultLogger, Env, Logger, Makefile};

/// An ordered list of filenames used to search for a makefile.
const MAKEFILE_SEARCH: [&str; 6] = [
    "makefile",
    "Makefile",
    "BSDmakefile",
    "BSDMakefile",
    "GNUmakefile",
    "GNUMakefile",
];

const LICENSE: &str = include_str!("../../../LICENSE");

/// Search for a makefile to execute.
///
/// We have to take into account that the file system may be case-insensitive. Ideally, we want to
/// return the proper casing of the makefile (so the file is properly reported when logging), and we
/// also want to support weirdly-cased makefiles on case-insensitive file systems, such as
/// `MAKEFILE`. To that end, we first get a directory listing and try to find makefiles from that
/// list, which would ensure the proper casing is returned. As a fallback, we then iterate through
/// the `MAKEFILE_SEARCH` list and try to read them from the file system, which will do a
/// case-insensitive match on case-insensitive file systems, and therefore would return improper
/// casing (e.g., `MAKEFILE` would be returned as `makefile`, since that would be the first match).
///
/// TODO: The first method of inspecting the directory listing is slower, and if that becomes an
/// issue, perhaps we only do that when verbose logging is enabled?
fn find_makefile() -> Option<PathBuf> {
    // First, try to find a makefile from the directory listing, which will be a case-sensitive
    // match. This ensures that if a case-sensitive match is found on a case-insensitive file
    // system, we will return the proper casing (e.g., if `Makefile` is found, then we won't have
    // first matched `makefile` and therefore returned the wrong casing).
    if let Some(cwd_files) = fs::read_dir("./").ok().map(|rd| {
        rd.flatten()
            .filter_map(|rd| rd.path().file_name().map(PathBuf::from))
            .collect::<Vec<_>>()
    }) {
        for file in MAKEFILE_SEARCH {
            let f = PathBuf::from(file);
            if cwd_files.contains(&f) && f.is_file() {
                return Some(f);
            }
        }
    }

    // Second, test each file in `MAKEFILE_SEARCH`, which then does a case-insensitive match on
    // case-insensitive file systems. This is purely for flexibility on case-insensitive file
    // systems (e.g., so a file named `MAKEFILE` would be matched), however it does result in the
    // "wrong" casing being logged.
    for file in MAKEFILE_SEARCH {
        let f = PathBuf::from(file);
        if f.is_file() {
            return Some(f);
        }
    }

    None
}

/// Print an error message and exit with code 2.
fn exit_with(msg: impl AsRef<str>, logger: &DefaultLogger, context: Option<&Context>) -> ! {
    logger.error(msg, context);
    std::process::exit(2)
}

fn main() {
    let args = Args::parse();
    let logger = DefaultLogger {};

    if args.license {
        println!("{}", LICENSE);
        return;
    }

    // Change to another directory, if specified by the arguments.
    let original_dir = if args.directory.is_empty() {
        None
    } else {
        // Remember the current directory to return to.
        let cwd = env::current_dir()
            .unwrap_or_else(|e| exit_with(format!("Failed to get cwd ({}).", e), &logger, None));

        // Change to the specified directory.
        let dir = args
            .directory
            .iter()
            .fold(PathBuf::new(), |dir, d| dir.join(d));
        logger.info(format!("Chdir to `{}`.", dir.display()), None);
        env::set_current_dir(&dir)
            .unwrap_or_else(|e| exit_with(format!("Chdir failed: {}.", e), &logger, None));

        Some(cwd)
    };

    // Determine the makefile to read.
    let makefile_fn = match args.file {
        None => find_makefile().unwrap_or_else(|| exit_with("No makefile found.", &logger, None)),
        Some(ref file) => PathBuf::from(file),
    };

    // TODO: Use `make_path` for sub-make invocations. Use `make_name` for logging rather than the
    // hardcoded `make`.
    //
    // // Determine how the current program was invoked. Note that we do not use `current_exe` because,
    // // at least on Linux, that would resolve symlinks, which is not what we want.
    // let make_path: String = env::args().next().unwrap();
    // let make_name: String = PathBuf::from(&make_path)
    //     .file_name()
    //     .unwrap()
    //     .to_string_lossy()
    //     .into();

    // Parse the makefile.
    let makefile = match Makefile::new(
        makefile_fn,
        args.clone().into(),
        Box::new(DefaultLogger {}),
        env::vars().collect::<Env>().into(),
    ) {
        Err(e) => exit_with(e.msg, &logger, Some(&e.context)),
        Ok(m) => m,
    };

    // Execute the makefile.
    if let Err(e) = makefile.execute(args.targets) {
        exit_with(e.msg, &logger, Some(&e.context));
    }

    // Go back to the original directory, if we changed directory previously.
    if let Some(cwd) = original_dir {
        logger.info(format!("Chdir back to `{}`.", cwd.display()), None);
        env::set_current_dir(&cwd)
            .unwrap_or_else(|e| exit_with(format!("Chdir failed: {}.", e), &logger, None));
    }
}
