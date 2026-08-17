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

use omake::{
    Compat, Context, DefaultLogger, Env, Logger, MakeError, Makefile, Opts, Origin, Vars, makeflags,
};

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
            'k' => {
                opts.keep_going = true;
                opts.no_keep_going = false;
            }
            'S' => {
                opts.keep_going = false;
                opts.no_keep_going = true;
            }
            'n' => opts.just_print = true,
            'i' => opts.ignore_errors = true,
            'B' => opts.always_make = true,
            's' => opts.silent = true,
            't' => opts.touch = true,
            'q' => opts.question = true,
            'e' => opts.environment_overrides = true,
            // `-R` (no built-in variables) implies `-r` (no built-in rules).
            'r' => opts.no_builtin_rules = true,
            'R' => {
                opts.no_builtin_rules = true;
                opts.no_builtin_variables = true;
            }
            _ => {}
        }
    }
}

/// Read the working-directory-message preference a parent make passed in `MAKEFLAGS`: `w` in the
/// leading short-flag word forces the "Entering/Leaving directory" messages on, a
/// `--no-print-directory` token forces them off. Returns `(force_on, force_off)`. Tokens after the
/// ` -- ` variables separator are ignored.
fn makeflags_print_directory(makeflags: &str) -> (bool, bool) {
    let (mut on, mut off) = (false, false);
    for (i, tok) in makeflags.split_whitespace().enumerate() {
        match tok {
            "--" => break,
            "--no-print-directory" => off = true,
            "--print-directory" => on = true,
            // The first bare (dashless) word is the short-flag cluster.
            _ if i == 0 && !tok.starts_with('-') && tok.contains('w') => on = true,
            _ => {}
        }
    }
    (on, off)
}

/// Extract the `-Idir` include-directory tokens a parent make passed in `MAKEFLAGS` (the part before
/// the ` -- ` variable separator), in order.
fn makeflags_include_dirs(makeflags: &str) -> Vec<String> {
    let head = makeflags.split(" -- ").next().unwrap_or(makeflags);
    head.split_whitespace()
        .filter_map(|t| t.strip_prefix("-I").map(str::to_string))
        .collect()
}

/// Recover the `-j` job count from a parent make's `MAKEFLAGS` (`-jN`, or `-j` alone for unlimited).
/// A `--jobserver-auth=` token (gmake's job pipe) is ignored — omake has no jobserver.
fn makeflags_jobs(makeflags: &str) -> Option<usize> {
    let head = makeflags.split(" -- ").next().unwrap_or(makeflags);
    head.split_whitespace()
        .find_map(|t| t.strip_prefix("-j").map(|n| n.parse().unwrap_or(0)))
}

/// Whether the leading short-flag word of a parent make's `MAKEFLAGS` contains `flag` (e.g. `s` for
/// `-s`). The first token is the bare (dashless) short-flag cluster; a token starting with `-` is a
/// long option, so there is no short-flag word.
fn makeflags_has_short_flag(makeflags: &str, flag: char) -> bool {
    makeflags
        .split_whitespace()
        .next()
        .is_some_and(|w| !w.starts_with('-') && w.contains(flag))
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

/// Fold a makefile's assignment to `MAKEFLAGS` back into the options and re-encode the variable in
/// canonical form. If the makefile never touched `MAKEFLAGS`, its value still equals the `startup`
/// encoding, so this returns immediately.
fn reconcile_makeflags<L: Logger>(makefile: &mut Makefile<L>, startup: &str, assignments: &[String]) {
    let current = makefile.vars.get("MAKEFLAGS").value.clone();
    if current == startup {
        return;
    }
    makeflags::merge_decoded(&mut makefile.opts, &current);
    let canonical = makeflags::encode(&makefile.opts, assignments);
    let _ = makefile
        .vars
        .set_origin("MAKEFLAGS", canonical.as_str(), false, Origin::File);
}

/// Print an error message and exit with code 2.
fn exit_with(logger: &DefaultLogger, msg: &str, context: Option<Context>) -> ! {
    logger.error(msg, context.as_ref());
    std::process::exit(2)
}

/// Report a `MakeError` and exit 2. Under `--compat`, an error may carry a pre-formatted
/// gmake/bmake-exact line (recipe failure, missing rule); any other error is rendered by
/// [`Compat::fatal_error`]. In native mode the error uses omake's `ERROR … | caret` form.
fn exit_make_error(logger: &DefaultLogger, compat: Compat, err: MakeError) -> ! {
    if let Some(line) = err.display {
        eprintln!("{line}");
        std::process::exit(2);
    }
    let file = err.context.path.as_ref().map(|p| p.display().to_string());
    let line = err.context.line_index.map(|l| l + 1);
    if let Some(rendered) = compat.fatal_error(&logger.prefix(), file.as_deref(), line, &err.msg) {
        eprintln!("{rendered}");
        std::process::exit(2);
    }
    exit_with(logger, &err.msg, Some(err.context))
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

    // `-v` prints the version, like `--version`.
    if args.show_version {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return;
    }

    // Resolve the program's own path for `$(MAKE)` before any `-C` chdir, so a relative invocation
    // path stays valid when a recipe recurses from another directory.
    let make_path = resolve_make_path();

    // GNU make prints "Entering/Leaving directory" around a `-C` chdir, unless `--no-print-directory`
    // (the messages are on by default when a directory is given, and `-w` forces them on). The
    // `-w`/`--no-print-directory` choice is inherited from a parent make via `MAKEFLAGS`, so a
    // top-level `--no-print-directory` reaches `$(MAKE)`-invoked children too.
    let parent_makeflags = env::var("MAKEFLAGS").unwrap_or_default();
    let (parent_w, parent_no_w) = makeflags_print_directory(&parent_makeflags);
    let force_print = args.print_directory || parent_w;
    let explicit_no_print = args.no_print_directory || parent_no_w;
    // `-s` (from the command line or inherited via `MAKEFLAGS`) also turns the directory messages
    // off, unless `-w`/`--print-directory` was given explicitly (which wins over `-s`). This is not
    // encoded as `--no-print-directory` in `MAKEFLAGS`: a child re-derives it from the inherited `s`.
    let silent = args.silent || makeflags_has_short_flag(&parent_makeflags, 's');
    let force_no_print = explicit_no_print || (silent && !force_print);
    // The messages are on for `-C`, `-w`, or a sub-make (`MAKELEVEL > 0`), unless turned off above.
    let print_directory =
        !force_no_print && (force_print || !args.directory.is_empty() || level > 0);

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

        Some(cwd)
    };

    // Announce the working directory (GNU make does this for `-C`, `-w`, and sub-makes), naming the
    // current directory even when no `-C` was given.
    if print_directory && let Ok(now) = env::current_dir() {
        println!(
            "{}: Entering directory '{}'",
            logger.prefix(),
            now.display()
        );
    }

    // Determine the makefile(s) to read: each `-f` in order, else the first found makefile.
    let makefile_fns: Vec<PathBuf> = if args.file.is_empty() {
        vec![find_makefile().unwrap_or_else(|| exit_with(&logger, "No makefile found.", None))]
    } else {
        args.file.iter().map(PathBuf::from).collect()
    };

    // Merge flags inherited from a parent make (via `MAKEFLAGS`), then re-encode the effective
    // flags for sub-makes. `MAKELEVEL` tracks the recursion depth.
    let mut opts: Opts = args.clone().into();
    apply_makeflags(&mut opts, &parent_makeflags);
    // A command-line `-k`/`-S` overrides an inherited one from `MAKEFLAGS` (clap already resolved
    // the two against each other, so at most one is set here).
    if args.keep_going {
        opts.keep_going = true;
        opts.no_keep_going = false;
    }
    if args.no_keep_going {
        opts.keep_going = false;
        opts.no_keep_going = true;
    }
    // A command-line `--no-silent` cancels an `-s` inherited via `MAKEFLAGS`.
    if args.no_silent {
        opts.silent = false;
    }
    // Inherit the parent make's `-I` include directories (honoring a `-I -` reset), appended after
    // any given on this command line.
    for dir in makeflags_include_dirs(&parent_makeflags) {
        if dir == "-" {
            opts.include_dirs.clear();
        } else {
            opts.include_dirs.push(dir);
        }
    }
    // Inherit a parent make's `-j` unless this command line set its own.
    if opts.jobs.is_none() {
        opts.jobs = makeflags_jobs(&parent_makeflags);
    }
    // Inherit the argument-bearing / long-only options a parent make passed (`-l`, `-O`, `--debug`,
    // `--trace`, ...) so they round-trip through this level's `MAKEFLAGS` too. The short cluster,
    // `-I`, and `-j` are already inherited above.
    makeflags::inherit_long_options(&mut opts, &parent_makeflags);
    // Record the resolved print-directory choice for `MAKEFLAGS` encoding (`w` when forced on, or
    // `--no-print-directory` when explicitly off).
    opts.print_dir_on = force_print;
    opts.print_dir_off = explicit_no_print;
    // `--compat` selects the diagnostic-output emulation; if the flag was absent, fall back to the
    // `OMAKE_COMPAT` environment variable (how a sub-make inherits it — it is not a real make flag,
    // so it does not travel in `MAKEFLAGS`).
    if opts.compat == Compat::Native
        && let Ok(v) = env::var("OMAKE_COMPAT")
    {
        opts.compat = match v.as_str() {
            "gnu" => Compat::Gnu,
            "bsd" => Compat::Bsd,
            _ => Compat::Native,
        };
    }
    // Export it so `$(MAKE)`-invoked children emulate the same make.
    if opts.compat != Compat::Native {
        let v = if opts.compat == Compat::Gnu { "gnu" } else { "bsd" };
        // SAFETY: single-threaded startup, before any recipe runs.
        unsafe { env::set_var("OMAKE_COMPAT", v) };
    }

    // A command-line token containing `=` is a variable assignment (`make FOO=bar`), not a goal.
    // These also arrive from a parent make via `MAKEFLAGS` (after `--`), so sub-makes inherit them.
    let (argv_assignments, goals): (Vec<String>, Vec<String>) = args
        .targets
        .iter()
        .cloned()
        .partition(|t| parse_cmdline_var(t).is_some());
    // For *setting* the variables, keep natural order (parent's first, then this command line's) so
    // a `:=` assignment sees earlier ones.
    let cmdline_tokens: Vec<String> = makeflags_assignments(&parent_makeflags)
        .into_iter()
        .chain(argv_assignments)
        .collect();
    // For *encoding* `MAKEFLAGS`, GNU make emits the assignments in reverse (a quirk of its internal
    // prepend-to-list). This flips the order at each `$(MAKE)` level, which gmake reproduces.
    let makeflags_assignments_enc: Vec<String> = cmdline_tokens.iter().rev().cloned().collect();

    // Encode MAKEFLAGS in GNU make's canonical order from the effective options, with the
    // command-line assignments after a ` -- ` separator (see `omake::makeflags`).
    let makeflags = makeflags::encode(&opts, &makeflags_assignments_enc);

    // Export MAKEFLAGS/MAKELEVEL to recipe children (and thus sub-makes). This runs once, at
    // startup on the main thread, before anything reads the environment concurrently.
    unsafe {
        env::set_var("MAKEFLAGS", &makeflags);
        env::set_var("MAKELEVEL", (level + 1).to_string());
    }

    // Build variables from the (updated) environment, then define `MAKE` (for `$(MAKE) -C subdir`)
    // and set `MAKELEVEL` to this level (children see it incremented, via the exported env above).
    // Under `-R` the built-in variables are omitted; under `-e` the environment outranks makefiles.
    let mut vars = Vars::from_env(
        env::vars().collect::<Env>(),
        !opts.no_builtin_variables,
        opts.environment_overrides,
    );
    // `MAKE` is a built-in default; `MAKELEVEL` comes from the environment (`$(origin ...)`).
    let _ = vars.set_origin("MAKE", make_path.as_str(), false, Origin::Default);
    // `CURDIR` is the current directory (after any `-C` chdir), like gmake.
    if let Ok(cwd) = env::current_dir() {
        let _ = vars.set_origin("CURDIR", &cwd.to_string_lossy(), false, Origin::File);
    }
    let _ = vars.set_origin(
        "MAKELEVEL",
        level.to_string().as_str(),
        false,
        Origin::Environment,
    );
    // `MAKECMDGOALS` holds the goals named on the command line (not variable assignments), in order.
    let _ = vars.set_origin("MAKECMDGOALS", &goals.join(" "), false, Origin::File);
    // `MAKEOVERRIDES` holds the command-line (and inherited) variable assignments, in the same
    // reversed order gmake emits them in `MAKEFLAGS`.
    let _ = vars.set_origin(
        "MAKEOVERRIDES",
        &makeflags_assignments_enc.join(" "),
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

    // `opts` is moved into the makefile below; remember whether question mode is on for the exit
    // code.
    let question = opts.question;
    let compat = opts.compat;

    // Parse the makefile.
    let mut makefile = match Makefile::new(makefile_fns, opts, vars, logger.clone()) {
        Err(e) => exit_make_error(&logger, compat, e),
        Ok(m) => m,
    };

    // GNU make treats `MAKEFLAGS` as a live switch set: a makefile that assigns to it turns those
    // switches on and the variable is reconstructed canonically. Fold any such assignment back into
    // the options and re-encode. An untouched `MAKEFLAGS` still equals the startup value, so this is
    // a no-op in the common case.
    reconcile_makeflags(&mut makefile, &makeflags, &makeflags_assignments_enc);

    // Execute the makefile (command-line variable assignments are not goals).
    let result = makefile.execute(goals);

    // Announce leaving the working directory (naming the current one), then return to the original
    // directory if `-C` changed it. This runs whether or not the build succeeded.
    if print_directory && let Ok(now) = env::current_dir() {
        println!("{}: Leaving directory '{}'", logger.prefix(), now.display());
    }
    if let Some(cwd) = original_dir {
        env::set_current_dir(&cwd)
            .unwrap_or_else(|e| exit_with(&logger, &format!("Chdir failed: {e}."), None));
    }

    match result {
        Err(e) => exit_make_error(&logger, compat, e),
        // `-q` (question mode): exit 1 if any target was found out of date, else 0.
        Ok(needs_rebuild) => {
            if question && needs_rebuild {
                std::process::exit(1);
            }
        }
    }
}
