//! Options available for makefiles.

#[derive(Debug, Default)]
pub struct Opts {
    /// Unconditionally make all targets.
    pub always_make: bool,

    /// Ignore errors from recipes.
    pub ignore_errors: bool,

    /// Don't execute recipes; just print them.
    pub just_print: bool,

    /// Consider FILE to be very old and do not remake it.
    pub old_file: Vec<String>,

    /// Consider FILE to be very new to simulate "what if" it changed.
    pub new_file: Vec<String>,
}
