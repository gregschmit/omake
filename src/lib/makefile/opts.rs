//! Options available for makefiles.

#[derive(Debug, Default)]
pub struct Opts {
    /// Unconditionally make all targets.
    pub always_make: bool,

    /// Ignore errors from recipes.
    pub ignore_errors: bool,

    /// Don't execute recipes; just print them.
    pub just_print: bool,

    /// Continue building unrelated targets after a recipe fails (`-k`).
    pub keep_going: bool,

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

    /// Consider FILE to be very old and do not remake it.
    pub old_file: Vec<String>,

    /// Consider FILE to be very new to simulate "what if" it changed.
    pub new_file: Vec<String>,
}
