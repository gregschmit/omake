//! A wrapper for a `HashMap` for storing the environment variables during makefile parsing.
//!
//! The only interesting behavior here is that for some special keys we have default values which
//! should be "resettable" by setting the value to blank, and that calling `get` on a key that
//! doesn't exist should return an empty `Var`. To support these behaviors without polluting the
//! underlying `HashMap` with lots of duplicate data, the `Vars` struct contains fields for those
//! heap-allocated "constant" objects. Since we always return a reference to a `Var`, this is quite
//! efficient.

use std::collections::HashMap;

const DEFAULT_RECIPE_PREFIX: char = '\t';
const BAD_VARIABLE_CHARS: [char; 3] = [':', '#', '='];
const DEFAULT_SUFFIXES: [&str; 13] = [
    ".C", ".F", ".S", ".c", ".cc", ".cpp", ".def", ".f", ".m", ".mod", ".p", ".r", ".s",
];

/// Variables which are set in a non-recursive context by default, and can be overridden by the
/// environment. `SHELL` is not included, since it cannot be overridden by the environment, unless
/// explicitly directed to by `-e`.
const DEFAULT_VARS: [(&str, &str); 21] = [
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
const DEFAULT_RECURSIVE_VARS: [(&str, &str); 36] = [
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

/// Represents the "raw" environment coming from the OS.
pub type Env = HashMap<String, String>;

#[derive(Debug)]
pub struct Var {
    pub value: String,
    pub recursive: bool,
}

/// This wraps a `HashMap` and a default value, providing an easy way to get variables, handling
/// special and automatic variables properly.
#[derive(Debug)]
pub struct Vars {
    map: HashMap<String, Var>,

    // Heap-allocated "constant" `Var` objects, setup during initialization, designed to reduce
    // multiple allocations and lifetime tracking.
    blank: Var,
    default_recipe_prefix: Var,
}

impl Vars {
    /// Primary interface for configuring a new instance. We also create some cached values that
    /// live for the lifetime of this instance to reduce the number of allocations.
    pub fn new<const N: usize>(init: [(&str, &str); N]) -> Self {
        let mut vars = Self {
            map: HashMap::new(),
            blank: Var {
                value: "".to_string(),
                recursive: false,
            },
            default_recipe_prefix: Var {
                value: DEFAULT_RECIPE_PREFIX.to_string(),
                recursive: false,
            },
        };

        // Set default vars.
        for (k, v) in DEFAULT_VARS {
            vars.set(k, v, false).unwrap();
        }

        // Set default recursive vars.
        for (k, v) in DEFAULT_RECURSIVE_VARS {
            vars.set(k, v, true).unwrap();
        }

        // Set `SHELL` to `/bin/sh` by default.
        vars.set("SHELL", "/bin/sh", false).unwrap();

        // Set default `SUFFIXES` and `.SUFFIXES`.
        vars.set("SUFFIXES", &DEFAULT_SUFFIXES.join(" "), false)
            .unwrap();
        vars.set(".SUFFIXES", &DEFAULT_SUFFIXES.join(" "), false)
            .unwrap();

        // Use `set` to initialize data.
        for (k, v) in init {
            let _ = vars.set(k, v, false);
        }

        vars
    }

    /// Public interface for getting variables. For unknown keys, the `blank` object is returned,
    /// and some special keys have default values.
    pub fn get(&self, k: impl AsRef<str>) -> &Var {
        let k = k.as_ref().trim();
        match k {
            ".RECIPEPREFIX" => match self.map.get(k) {
                None => &self.default_recipe_prefix,
                Some(var) => {
                    if var.value.is_empty() {
                        &self.default_recipe_prefix
                    } else {
                        var
                    }
                }
            },
            _ => match self.map.get(k) {
                None => &self.blank,
                Some(var) => var,
            },
        }
    }

    /// Public interface for setting variables.
    pub fn set<S: Into<String>>(&mut self, k: S, v: S, recursive: bool) -> Result<(), String> {
        let k = k.into().trim().to_string();
        let v = v.into();

        // Do not insert bad variable names.
        for ch in k.chars() {
            if ch.is_whitespace() {
                return Err("Variable contains whitespace.".to_string());
            }

            if BAD_VARIABLE_CHARS.contains(&ch) {
                return Err(format!("Variable contains bad character '{}'.", ch));
            }
        }

        self.map.insert(
            k,
            Var {
                value: v,
                recursive,
            },
        );
        Ok(())
    }
}

impl From<Env> for Vars {
    fn from(env: Env) -> Self {
        let mut vars = Self::new([]);

        for (k, v) in env {
            // Do not load `SHELL` from the environment.
            if k == "SHELL" {
                continue;
            }

            vars.map.insert(
                k,
                Var {
                    value: v,
                    recursive: false,
                },
            );
        }

        vars
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
}
