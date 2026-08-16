//! # omake
//!
//! This is an implementation of `make`, written in Rust. The goal is to provide an implementation
//! of `make` that can be used to process both BSD and GNU makefiles.

mod args;

use std::env;
use std::fs;
use std::path::PathBuf;

use clap::Parser;

use args::Args;

use omake::{Context, DefaultLogger, Env, Logger, Makefile, Opts, Origin, Vars};

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

/// The program's invocation name (argv[0] basename), used as the message prefix like GNU make
/// (`make: ...`, or `omake: ...` when invoked as `omake`).
fn program_name() -> String {
    env::args()
        .next()
        .as_deref()
        .and_then(|a| std::path::Path::new(a).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "make".to_string())
}

/// The program's own invocation path, for `$(MAKE)`. A relative path that contains a separator is
/// made absolute against the current directory (so it survives a later `-C` chdir); a bare
/// PATH-resolved name (e.g. `omake`) is left as-is. We avoid `current_exe` because, at least on
/// Linux, it resolves symlinks, which is not what we want.
fn resolve_make_path() -> String {
    let argv0 = env::args().next().unwrap_or_else(|| "omake".to_string());
    let path = PathBuf::from(&argv0);

    if path.is_relative()
        && argv0.contains(std::path::MAIN_SEPARATOR)
        && let Ok(cwd) = env::current_dir()
    {
        return cwd.join(path).to_string_lossy().into_owned();
    }
    argv0
}

/// Apply single-letter flags inherited from a parent make's `MAKEFLAGS` to `opts`. Only the leading
/// short-flag word is scanned; it is bare (no leading dash), so a token starting with `-` is a long
/// option (e.g. `--no-print-directory`) and must not be read character-by-character.
fn apply_makeflags(opts: &mut Opts, makeflags: &str) {
    let short = makeflags.split_whitespace().next().unwrap_or("");
    if short.starts_with('-') {
        return;
    }
    for c in short.chars() {
        match c {
            'k' => opts.keep_going = true,
            'n' => opts.just_print = true,
            'i' => opts.ignore_errors = true,
            'B' => opts.always_make = true,
            _ => {}
        }
    }
}

/// Parse a command-line variable assignment token (`FOO=bar`, `FOO:=bar`). Returns the name,
/// whether the flavor is recursive (`=` yes, `:=`/`::=` no), and the value. Returns `None` if the
/// token is not an assignment (no `=`, or an empty name).
fn parse_cmdline_var(token: &str) -> Option<(&str, bool, &str)> {
    let eq = token.find('=')?;
    let value = &token[eq + 1..];
    let mut name = &token[..eq];
    // A trailing operator char before `=` selects the flavor; `:=`/`::=` are simple.
    let recursive = if name.ends_with(':') {
        name = name.trim_end_matches(':');
        false
    } else {
        name = name.trim_end_matches(['+', '?', '!']);
        true
    };
    let name = name.trim();
    (!name.is_empty()).then_some((name, recursive, value))
}

/// Backslash-escape whitespace (and the backslash itself) so a `MAKEFLAGS` assignment token
/// survives whitespace-splitting when a sub-make re-parses it.
fn escape_makeflag(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    for c in token.chars() {
        if c == '\\' || c.is_whitespace() {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Extract the variable-assignment tokens from a `MAKEFLAGS` string (everything after the ` -- `
/// separator), un-escaping each. This is how a parent make passes command-line variables down.
fn makeflags_assignments(makeflags: &str) -> Vec<String> {
    let Some((_, rest)) = makeflags.split_once(" -- ") else {
        return Vec::new();
    };
    // Split on unescaped whitespace, un-escaping `\<char>` to `<char>`.
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Encode the effective single-letter flags as `MAKEFLAGS` for sub-makes (e.g. `-k -n` -> "kn").
fn encode_makeflags(opts: &Opts) -> String {
    let mut flags = String::new();
    if opts.ignore_errors {
        flags.push('i');
    }
    if opts.keep_going {
        flags.push('k');
    }
    if opts.just_print {
        flags.push('n');
    }
    if opts.always_make {
        flags.push('B');
    }
    flags
}

/// Print an error message and exit with code 2.
fn exit_with(logger: &DefaultLogger, msg: &str, context: Option<Context>) -> ! {
    logger.error(msg, context.as_ref());
    std::process::exit(2)
}

fn main() {
    let args = Args::parse();
    // The recursion level (`MAKELEVEL`) and invocation name feed the message prefix (`make[N]:`).
    let level: u32 = env::var("MAKELEVEL")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let logger = DefaultLogger {
        program: program_name(),
        level,
    };

    if args.license {
        println!("{LICENSE}");
        return;
    }

    // Resolve the program's own path for `$(MAKE)` before any `-C` chdir, so a relative invocation
    // path stays valid when a recipe recurses from another directory.
    let make_path = resolve_make_path();

    // GNU make prints "Entering/Leaving directory" around a `-C` chdir, unless `--no-print-directory`
    // (the messages are on by default when a directory is given, and `-w` forces them on).
    let print_directory =
        !args.no_print_directory && (args.print_directory || !args.directory.is_empty());

    // Change to another directory, if specified by the arguments.
    let original_dir = if args.directory.is_empty() {
        None
    } else {
        // Remember the current directory to return to.
        let cwd = env::current_dir()
            .unwrap_or_else(|e| exit_with(&logger, &format!("Failed to get cwd ({e})."), None));

        // Change to the specified directory.
        let dir = args
            .directory
            .iter()
            .fold(PathBuf::new(), |dir, d| dir.join(d));
        env::set_current_dir(&dir)
            .unwrap_or_else(|e| exit_with(&logger, &format!("Chdir failed: {e}."), None));
        if print_directory && let Ok(now) = env::current_dir() {
            println!(
                "{}: Entering directory '{}'",
                logger.prefix(),
                now.display()
            );
        }

        Some(cwd)
    };

    // Determine the makefile to read.
    let makefile_fn = match args.file {
        None => find_makefile().unwrap_or_else(|| exit_with(&logger, "No makefile found.", None)),
        Some(ref file) => PathBuf::from(file),
    };

    // Merge flags inherited from a parent make (via `MAKEFLAGS`), then re-encode the effective
    // flags for sub-makes. `MAKELEVEL` tracks the recursion depth.
    let mut opts: Opts = args.clone().into();
    let parent_makeflags = env::var("MAKEFLAGS").unwrap_or_default();
    apply_makeflags(&mut opts, &parent_makeflags);

    // A command-line token containing `=` is a variable assignment (`make FOO=bar`), not a goal.
    // These also arrive from a parent make via `MAKEFLAGS` (after `--`), so sub-makes inherit them.
    let (cmdline_tokens, goals): (Vec<String>, Vec<String>) = args
        .targets
        .iter()
        .cloned()
        .partition(|t| parse_cmdline_var(t).is_some());
    let cmdline_tokens: Vec<String> = makeflags_assignments(&parent_makeflags)
        .into_iter()
        .chain(cmdline_tokens)
        .collect();

    // Re-encode MAKEFLAGS with the command-line assignments after a `--` separator. Whitespace
    // inside a value is backslash-escaped so a sub-make can recover it (`make 'FOO=a b'`).
    let mut makeflags = encode_makeflags(&opts);
    if !cmdline_tokens.is_empty() {
        makeflags.push_str(" -- ");
        let escaped: Vec<String> = cmdline_tokens.iter().map(|t| escape_makeflag(t)).collect();
        makeflags.push_str(&escaped.join(" "));
    }

    // Export MAKEFLAGS/MAKELEVEL to recipe children (and thus sub-makes). This runs once, at
    // startup on the main thread, before anything reads the environment concurrently.
    unsafe {
        env::set_var("MAKEFLAGS", &makeflags);
        env::set_var("MAKELEVEL", (level + 1).to_string());
    }

    // Build variables from the (updated) environment, then define `MAKE` (for `$(MAKE) -C subdir`)
    // and set `MAKELEVEL` to this level (children see it incremented, via the exported env above).
    let mut vars: Vars = env::vars().collect::<Env>().into();
    // `MAKE` is a built-in default; `MAKELEVEL` comes from the environment (`$(origin ...)`).
    let _ = vars.set_origin("MAKE", make_path.as_str(), false, Origin::Default);
    let _ = vars.set_origin(
        "MAKELEVEL",
        level.to_string().as_str(),
        false,
        Origin::Environment,
    );
    // Command-line variables override makefile assignments (via origin precedence) and are
    // auto-exported to recipes. A `:=` (simple) value is expanded once, immediately.
    for token in &cmdline_tokens {
        if let Some((name, recursive, value)) = parse_cmdline_var(token) {
            let stored = if recursive {
                value.to_string()
            } else {
                omake::expand::expand(value, &vars).unwrap_or_else(|_| value.to_string())
            };
            let _ = vars.set_origin(name, stored.as_str(), recursive, Origin::CommandLine);
        }
    }

    // Parse the makefile.
    let makefile = match Makefile::new(makefile_fn, opts, vars, logger.clone()) {
        Err(e) => exit_with(&logger, &e.msg, Some(e.context)),
        Ok(m) => m,
    };

    // Execute the makefile (command-line variable assignments are not goals).
    if let Err(e) = makefile.execute(goals) {
        exit_with(&logger, &e.msg, Some(e.context));
    }

    // Go back to the original directory, if we changed directory previously. The "Leaving" message
    // names the directory being left (the current one), not the one returned to.
    if let Some(cwd) = original_dir {
        if print_directory && let Ok(now) = env::current_dir() {
            println!("{}: Leaving directory '{}'", logger.prefix(), now.display());
        }
        env::set_current_dir(&cwd)
            .unwrap_or_else(|e| exit_with(&logger, &format!("Chdir failed: {e}."), None));
    }
}
