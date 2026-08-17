//! Options available for makefiles.

use crate::compat::Compat;

#[derive(Debug, Default, Clone)]
pub struct Opts {
    /// The make whose diagnostic output to emulate (`--compat`); `Native` by default.
    pub compat: Compat,

    /// Unconditionally make all targets.
    pub always_make: bool,

    /// Ignore errors from recipes.
    pub ignore_errors: bool,

    /// Don't execute recipes; just print them.
    pub just_print: bool,

    /// Continue building unrelated targets after a recipe fails (`-k`).
    pub keep_going: bool,

    /// `-S`/`--no-keep-going` was the effective choice (turns `-k` off; emitted as `S` in
    /// `MAKEFLAGS` so a sub-make inherits the override of an ancestor's `-k`).
    pub no_keep_going: bool,

    /// Do not echo recipe commands before running them (`-s`, like a global `.SILENT:`).
    pub silent: bool,

    /// Disable the built-in implicit rules (`-r`); also clears the default suffix list.
    pub no_builtin_rules: bool,

    /// Do not define the built-in variables (`-R`); implies `no_builtin_rules`.
    pub no_builtin_variables: bool,

    /// Let environment variables override makefile assignments (`-e`).
    pub environment_overrides: bool,

    /// Touch targets (mark them up to date) instead of running their recipes (`-t`).
    pub touch: bool,

    /// Directories searched for `include`d makefiles (`-I`), in order, after the current directory.
    pub include_dirs: Vec<String>,

    /// Question mode (`-q`): run no recipes; exit 0 if all goals are up to date, 1 if any needs
    /// rebuilding, 2 on error.
    pub question: bool,

    /// Requested parallel jobs (`-j`): `None` = serial, `Some(0)` = unlimited, `Some(n)` = n. omake
    /// currently builds serially regardless; kept for the `MAKEFLAGS` round-trip.
    pub jobs: Option<usize>,

    /// Output-sync mode (`-O`): the raw mode word (`none`, `target`, `line`, `recurse`). omake is
    /// serial, so this only round-trips through `MAKEFLAGS`.
    pub output_sync: Option<String>,

    /// Load-average limit (`-l`): the raw value (e.g. `2.5`), preserved for the `MAKEFLAGS`
    /// round-trip. omake is serial, so it has no other effect.
    pub load_average: Option<String>,

    /// Debug flags from `--debug=FLAGS` (accepted; `MAKEFLAGS` round-trip only). Bare `-d` sets
    /// [`Opts::debug_all`] instead, which round-trips as a `d` in the short-flag cluster.
    pub debug: Option<String>,

    /// Bare `-d` (debug everything). Emitted as `d` in the `MAKEFLAGS` short-flag cluster.
    pub debug_all: bool,

    /// `--trace` (accepted; `MAKEFLAGS` round-trip only).
    pub trace: bool,

    /// `--warn-undefined-variables` (accepted; `MAKEFLAGS` round-trip only).
    pub warn_undefined_variables: bool,

    /// Emit `w` in the `MAKEFLAGS` short-flag cluster (an explicit `-w`/`--print-directory`).
    pub print_dir_on: bool,

    /// Emit `--no-print-directory` in `MAKEFLAGS` (an explicit `--no-print-directory`).
    pub print_dir_off: bool,

    /// Consider FILE to be very old and do not remake it.
    pub old_file: Vec<String>,

    /// Consider FILE to be very new to simulate "what if" it changed.
    pub new_file: Vec<String>,
}
