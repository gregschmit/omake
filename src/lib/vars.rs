//! A wrapper for a [`HashMap`] storing the environment variables during makefile parsing.
//!
//! The only interesting behavior here is that for some special keys we have default values which
//! should be "resettable" by setting the value to blank, and that calling `get` on a key that
//! doesn't exist should return an empty [`Var`]. To support these behaviors without polluting the
//! underlying `HashMap` with lots of duplicate data, the [`Vars`] struct contains fields for those
//! heap-allocated "constant" objects. Since we always return a reference to a [`Var`], this is
//! quite efficient.

use std::collections::HashMap;

pub const BAD_VARIABLE_CHARS: [char; 3] = [':', '#', '='];
// GNU make's exact default `.SUFFIXES` list and order (from `gmake -p -f /dev/null`). Drives both
// the `SUFFIXES` snapshot variable and internal suffix-rule recognition.
pub const DEFAULT_SUFFIXES: [&str; 35] = [
    ".out", ".a", ".ln", ".o", ".c", ".cc", ".C", ".cpp", ".p", ".f", ".F", ".m", ".r", ".y", ".l",
    ".ym", ".yl", ".s", ".S", ".mod", ".sym", ".def", ".h", ".info", ".dvi", ".tex", ".texinfo",
    ".texi", ".txinfo", ".w", ".ch", ".web", ".sh", ".elc", ".el",
];

/// List of variables where setting the value to blank means to reset it to the default value. All
/// of these values MUST exist in [`DEFAULT_VARS`].
pub const BLANK_MEANS_DEFAULT_VARS: [&str; 1] = [".RECIPEPREFIX"];

/// Variables which are set in a non-recursive context by default, and can be overridden by the
/// environment. `SHELL` is not included, since it cannot be overridden by the environment,
/// unless explicitly directed to by `-e`.
pub const DEFAULT_VARS: [(&str, &str); 23] = [
    (".RECIPEPREFIX", "\t"),
    (".SHELLFLAGS", "-c"),
    ("AR", "ar"),
    ("ARFLAGS", "rv"),
    ("AS", "as"),
    ("CC", "cc"),
    ("CO", "co"),
    ("CTANGLE", "ctangle"),
    ("CWEAVE", "cweave"),
    ("CXX", "c++"),
    ("FC", "f77"),
    ("GET", "get"),
    ("LD", "ld"),
    ("LEX", "lex"),
    ("LINT", "lint"),
    ("M2C", "m2c"),
    ("OBJC", "cc"),
    ("PC", "pc"),
    ("RM", "rm -f"),
    ("TANGLE", "tangle"),
    ("TEX", "tex"),
    ("WEAVE", "weave"),
    ("YACC", "yacc"),
];

/// Variables which are set in a recursive context by default, and can be overridden by the
/// environment.
#[rustfmt::skip]
pub const DEFAULT_RECURSIVE_VARS: [(&str, &str); 36] = [
    ("OUTPUT_OPTION", "-o $@"),

    // Compiler definitions.
    ("CPP", "$(CC) -E"),
    ("F77", "$(FC)"),
    ("F77FLAGS", "$(FFLAGS)"),
    ("LEX.m", "$(LEX) $(LFLAGS) -t"),
    ("YACC.m", "$(YACC) $(YFLAGS)"),
    ("YACC.y", "$(YACC) $(YFLAGS)"),

    // Implicit rule definitions.
    ("COMPILE.C", "$(COMPILE.cc)"),
    ("COMPILE.F", "$(FC) $(FFLAGS) $(CPPFLAGS) $(TARGET_ARCH) -c"),
    ("COMPILE.S", "$(CC) $(ASFLAGS) $(CPPFLAGS) $(TARGET_MACH) -c"),
    ("COMPILE.c", "$(CC) $(CFLAGS) $(CPPFLAGS) $(TARGET_ARCH) -c"),
    ("COMPILE.cc", "$(CXX) $(CXXFLAGS) $(CPPFLAGS) $(TARGET_ARCH) -c"),
    ("COMPILE.cpp", "$(COMPILE.cc)"),
    ("COMPILE.def", "$(M2C) $(M2FLAGS) $(DEFFLAGS) $(TARGET_ARCH)"),
    ("COMPILE.f", "$(FC) $(FFLAGS) $(TARGET_ARCH) -c"),
    ("COMPILE.m", "$(OBJC) $(OBJCFLAGS) $(CPPFLAGS) $(TARGET_ARCH) -c"),
    ("COMPILE.mod", "$(M2C) $(M2FLAGS) $(MODFLAGS) $(TARGET_ARCH)"),
    ("COMPILE.p", "$(PC) $(PFLAGS) $(CPPFLAGS) $(TARGET_ARCH) -c"),
    ("COMPILE.r", "$(FC) $(FFLAGS) $(RFLAGS) $(TARGET_ARCH) -c"),
    ("COMPILE.s", "$(AS) $(ASFLAGS) $(TARGET_MACH)"),
    ("LINK.C", "$(LINK.cc)"),
    ("LINK.F", "$(FC) $(FFLAGS) $(CPPFLAGS) $(LDFLAGS) $(TARGET_ARCH)"),
    ("LINK.S", "$(CC) $(ASFLAGS) $(CPPFLAGS) $(LDFLAGS) $(TARGET_MACH)"),
    ("LINK.c", "$(CC) $(CFLAGS) $(CPPFLAGS) $(LDFLAGS) $(TARGET_ARCH)"),
    ("LINK.cc", "$(CXX) $(CXXFLAGS) $(CPPFLAGS) $(LDFLAGS) $(TARGET_ARCH)"),
    ("LINK.cpp", "$(LINK.cc)"),
    ("LINK.f", "$(FC) $(FFLAGS) $(LDFLAGS) $(TARGET_ARCH)"),
    ("LINK.m", "$(OBJC) $(OBJCFLAGS) $(CPPFLAGS) $(LDFLAGS) $(TARGET_ARCH)"),
    ("LINK.o", "$(CC) $(LDFLAGS) $(TARGET_ARCH)"),
    ("LINK.p", "$(PC) $(PFLAGS) $(CPPFLAGS) $(LDFLAGS) $(TARGET_ARCH)"),
    ("LINK.r", "$(FC) $(FFLAGS) $(RFLAGS) $(LDFLAGS) $(TARGET_ARCH)"),
    ("LINK.s", "$(CC) $(ASFLAGS) $(LDFLAGS) $(TARGET_MACH)"),
    ("LINT.c", "$(LINT) $(LINTFLAGS) $(CPPFLAGS) $(TARGET_ARCH)"),
    ("PREPROCESS.F", "$(FC) $(FFLAGS) $(CPPFLAGS) $(TARGET_ARCH) -F"),
    ("PREPROCESS.S", "$(CC) -E $(CPPFLAGS)"),
    ("PREPROCESS.r", "$(FC) $(FFLAGS) $(RFLAGS) $(TARGET_ARCH) -F"),
];

/// The "raw" environment variables, coming from the OS.
pub type Env = HashMap<String, String>;

/// Where a variable's definition came from, reported by `$(origin ...)`. Automatic variables are
/// recognized by name in the `origin` function rather than stored here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A built-in default (e.g. `CC`, `MAKE`).
    Default,
    /// Inherited from the process environment.
    Environment,
    /// Assigned in a makefile.
    File,
    /// Inherited from the environment under `-e`, which ranks the environment above makefile
    /// assignments (but still below the command line and `override`).
    EnvironmentOverride,
    /// Assigned on the command line (`make FOO=bar`).
    CommandLine,
    /// Assigned with the `override` directive.
    Override,
}

impl Origin {
    /// Precedence rank: an assignment of a higher-ranked origin overrides one of a lower rank (and
    /// a lower-ranked assignment is a no-op against a higher-ranked binding). This is how a
    /// command-line variable beats a file assignment, and `override` beats the command line.
    pub fn rank(self) -> u8 {
        match self {
            Origin::Default => 0,
            Origin::Environment => 1,
            Origin::File => 2,
            Origin::EnvironmentOverride => 3,
            Origin::CommandLine => 4,
            Origin::Override => 5,
        }
    }
}

/// A single variable: its value, whether it is recursively expanded, and where it came from.
#[derive(Clone, Debug)]
pub struct Var {
    pub value: String,
    pub recursive: bool,
    pub origin: Origin,
    /// Whether the variable is exported into recipe child environments (set by a target-specific
    /// `export VAR = value`); global exports are tracked separately on the `Makefile`.
    pub exported: bool,
}

/// Wrap a [`HashMap`] and a default `blank` value, providing an easy way to get variables, handling
/// special and automatic variables properly.
#[derive(Clone, Debug)]
pub struct Vars {
    map: HashMap<String, Var>,

    /// Variable to return when a variable is not found. This is allocated during initialization to
    /// prevent multiple blank allocations in the map and lifetime tracking.
    blank: Var,

    /// Stashing a map of [`DEFAULT_VARS`] here to make lookup fast since we sometimes need to
    /// revert a value back to the default.
    default_vars: HashMap<String, String>,

    /// Under `-e`, environment variables outrank makefile assignments (see
    /// [`set_origin`](Self::set_origin)).
    env_override: bool,
}

impl Vars {
    /// Primary interface for configuring a new instance. We also create some cached values that
    /// live for the lifetime of this instance to reduce the number of allocations.
    pub fn new<const N: usize>(init: [(&str, &str); N]) -> Self {
        Self::new_inner(init, true)
    }

    /// Like [`new`](Self::new), but omits make's built-in tool variables (`CC`, `CFLAGS`,
    /// `COMPILE.c`, ...) for `-R`/`--no-builtin-variables`. The operational specials
    /// (`.RECIPEPREFIX`, `.SHELLFLAGS`, `SHELL`, and the suffix list) are always defined so make can
    /// still run recipes; env/makefile/command-line definitions are unaffected.
    pub fn new_without_builtins<const N: usize>(init: [(&str, &str); N]) -> Self {
        Self::new_inner(init, false)
    }

    fn new_inner<const N: usize>(init: [(&str, &str); N], builtins: bool) -> Self {
        let mut vars = Self {
            map: HashMap::new(),
            blank: Var {
                value: "".to_string(),
                recursive: false,
                origin: Origin::File,
                exported: false,
            },
            default_vars: HashMap::new(),
            env_override: false,
        };

        // Set default vars. The dotted specials (`.RECIPEPREFIX`, `.SHELLFLAGS`) are always defined;
        // the tool vars (`CC`, `AR`, ...) are skipped under `-R`. The `default_vars` cache is always
        // populated so blank-means-default keys (`.RECIPEPREFIX`) can reset.
        for (k, v) in DEFAULT_VARS {
            if builtins || k.starts_with('.') {
                vars.set_origin(k, v, false, Origin::Default).unwrap();
            }
            vars.default_vars.insert(k.to_string(), v.to_string());
        }

        // Set default recursive vars (all tool/compile definitions — skipped under `-R`).
        if builtins {
            for (k, v) in DEFAULT_RECURSIVE_VARS {
                vars.set_origin(k, v, true, Origin::Default).unwrap();
            }
        }

        // Set `SHELL` to `/bin/sh` by default.
        vars.set_origin("SHELL", "/bin/sh", false, Origin::Default)
            .unwrap();

        // Set the default `SUFFIXES` snapshot variable. `.SUFFIXES` is a special *target*, not a
        // variable (so `$(.SUFFIXES)` is empty), matching gmake — its prereq list lives in the
        // makefile's `suffixes` field.
        let suffixes = DEFAULT_SUFFIXES.join(" ");
        vars.set_origin("SUFFIXES", &suffixes, false, Origin::Default)
            .unwrap();

        // Use `set` to initialize data (treated as file-origin, like a makefile assignment).
        for (k, v) in init {
            let _ = vars.set(k, v, false);
        }

        vars
    }

    /// Public interface for getting variables. For unknown keys, the `blank` object is returned. We
    /// should try to keep this interface as fast/simple as possible since it's used far more often
    /// than `set` (e.g., used for each line to check for recipe prefix).
    pub fn get(&self, k: impl AsRef<str>) -> &Var {
        let k = k.as_ref().trim();

        match self.map.get(k) {
            None => &self.blank,
            Some(var) => var,
        }
    }

    /// Whether a variable is currently defined (including defaults and environment). Used to
    /// implement `?=` (set-if-undefined) and to pick `+=`'s behavior.
    pub fn is_set(&self, k: impl AsRef<str>) -> bool {
        self.map.contains_key(k.as_ref().trim())
    }

    /// Iterate over every defined variable as `(name, &Var)`. Used by export-all and `$(...)`
    /// introspection.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Var)> {
        self.map.iter()
    }

    /// Remove a variable (for the `undefine` directive). Returns its prior [`Var`], if any.
    pub fn remove(&mut self, k: impl AsRef<str>) -> Option<Var> {
        self.map.remove(k.as_ref().trim())
    }

    /// Remove the positional call parameters (`$(0)`, `$(1)`, ...): every variable whose name is
    /// all ASCII digits. `$(call ...)` uses this so a nested call does not inherit higher-numbered
    /// parameters from the enclosing call.
    pub fn clear_call_params(&mut self) {
        self.map
            .retain(|k, _| k.is_empty() || !k.bytes().all(|b| b.is_ascii_digit()));
    }

    /// Public interface for setting variables. The origin is recorded as `File` (a makefile
    /// assignment); use [`Vars::set_origin`] for other origins.
    pub fn set<S: Into<String>>(&mut self, k: S, v: S, recursive: bool) -> Result<(), String> {
        self.set_origin(k, v, recursive, Origin::File)
    }

    /// Set a variable, recording where it came from (for `$(origin ...)`).
    pub fn set_origin<S: Into<String>>(
        &mut self,
        k: S,
        v: S,
        recursive: bool,
        origin: Origin,
    ) -> Result<(), String> {
        let k = k.into().trim().to_string();
        let mut v = v.into();

        // Do not insert bad variable names.
        for ch in k.chars() {
            if ch.is_whitespace() {
                return Err("Variable contains whitespace.".to_string());
            }

            if BAD_VARIABLE_CHARS.contains(&ch) {
                return Err(format!("Variable contains bad character '{ch}'."));
            }
        }

        // Under `-e`, an environment variable outranks an unconditional makefile assignment: keep
        // the environment value but record that a makefile assignment was overridden (so
        // `$(origin)` reports "environment override"). A `?=`/conditional that finds the variable
        // already set never reaches here, so a pure environment variable keeps "environment".
        if self.env_override
            && origin == Origin::File
            && let Some(existing) = self.map.get_mut(&k)
            && existing.origin == Origin::Environment
        {
            existing.origin = Origin::EnvironmentOverride;
            return Ok(());
        }

        // Precedence: a lower-ranked origin cannot overwrite a higher-ranked binding (so a makefile
        // assignment is a no-op against a command-line or `override` variable). Equal ranks
        // overwrite, allowing normal reassignment.
        if let Some(existing) = self.map.get(&k)
            && existing.origin.rank() > origin.rank()
        {
            return Ok(());
        }

        if BLANK_MEANS_DEFAULT_VARS.contains(&&k[..]) && v.is_empty() {
            v = self.default_vars.get(&k).unwrap().to_string();
        }

        // Reassigning a variable keeps its exported flag (an exported variable stays exported).
        let exported = self.map.get(&k).is_some_and(|existing| existing.exported);
        self.map.insert(
            k,
            Var {
                value: v,
                recursive,
                origin,
                exported,
            },
        );
        Ok(())
    }

    /// Mark a variable exported into recipe child environments (a target-specific `export VAR`). The
    /// variable is expected to already exist; if not, this is a no-op.
    pub fn set_exported(&mut self, k: impl AsRef<str>) {
        if let Some(var) = self.map.get_mut(k.as_ref()) {
            var.exported = true;
        }
    }
}

impl Vars {
    /// Build the variable set from the OS environment. `builtin_vars` is false under `-R`, which
    /// omits make's built-in variables so they do not shadow (or appear as) real definitions.
    /// `env_override` is true under `-e`, which ranks environment variables above makefile
    /// assignments: a later unconditional makefile assignment to an environment variable is rejected
    /// and its origin becomes `EnvironmentOverride` (see [`set_origin`](Self::set_origin)).
    pub fn from_env(env: Env, builtin_vars: bool, env_override: bool) -> Self {
        let mut vars = if builtin_vars {
            Self::new([])
        } else {
            Self::new_without_builtins([])
        };
        vars.env_override = env_override;

        for (k, v) in env {
            // Don't ever load `SHELL` from the environment.
            if k == "SHELL" {
                continue;
            }

            // Route through `set_origin` so environment variables get the same validation and
            // special-key handling (e.g. blank-means-default) as makefile assignments. Skip invalid
            // names.
            let _ = vars.set_origin(k, v, false, Origin::Environment);
        }

        vars
    }
}

impl From<Env> for Vars {
    fn from(env: Env) -> Self {
        Self::from_env(env, true, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_and_default_values() {
        let vars = Vars::new([("A", "B")]);
        assert_eq!(vars.get("A").value, "B");
        assert_eq!(vars.get("B").value, "");
        assert_eq!(vars.get("SHELL").value, "/bin/sh");
        assert_eq!(
            vars.get("COMPILE.c").value,
            "$(CC) $(CFLAGS) $(CPPFLAGS) $(TARGET_ARCH) -c"
        );
    }

    #[test]
    fn test_recipe_prefix() {
        let mut vars = Vars::new([]);
        assert_eq!(vars.get(".RECIPEPREFIX").value, "\t");
        vars.set(".RECIPEPREFIX", "B", false).unwrap();
        assert_eq!(vars.get(".RECIPEPREFIX").value, "B");
        vars.set(".RECIPEPREFIX", "", false).unwrap();
        assert_eq!(vars.get(".RECIPEPREFIX").value, "\t");
    }

    #[test]
    fn test_no_builtins_omits_tool_vars() {
        // `-R`: tool variables are absent, but the operational specials remain.
        let vars = Vars::new_without_builtins([]);
        assert_eq!(vars.get("CC").value, "");
        assert_eq!(vars.get("CC").origin, Origin::File); // the `blank` fallback
        assert_eq!(vars.get("COMPILE.c").value, "");
        assert_eq!(vars.get(".RECIPEPREFIX").value, "\t");
        assert_eq!(vars.get("SHELL").value, "/bin/sh");
    }

    #[test]
    fn test_env_override() {
        // `-e`: an environment variable outranks an unconditional makefile assignment, which is
        // rejected and re-tagged `EnvironmentOverride`. A pure environment variable stays
        // `Environment`; the command line still wins.
        let env: Env = [("FROMFILE", "env"), ("PUREENV", "env")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut vars = Vars::from_env(env, true, true);

        vars.set("FROMFILE", "file", false).unwrap(); // makefile `=`, rejected under -e
        assert_eq!(vars.get("FROMFILE").value, "env");
        assert_eq!(vars.get("FROMFILE").origin, Origin::EnvironmentOverride);

        assert_eq!(vars.get("PUREENV").origin, Origin::Environment);

        vars.set_origin("FROMFILE", "cmd", false, Origin::CommandLine)
            .unwrap();
        assert_eq!(vars.get("FROMFILE").value, "cmd");
        assert_eq!(vars.get("FROMFILE").origin, Origin::CommandLine);
    }

    #[test]
    fn test_no_env_override_by_default() {
        // Without `-e`, a makefile assignment beats the environment (the default).
        let env: Env = [("V", "env")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut vars = Vars::from_env(env, true, false);
        vars.set("V", "file", false).unwrap();
        assert_eq!(vars.get("V").value, "file");
        assert_eq!(vars.get("V").origin, Origin::File);
    }
}
