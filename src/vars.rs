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
    make_var: Var,
    makeargs_var: Var,
}

impl Vars {
    /// Primary interface for configuring a new instance. We also create some cached values that
    /// live for the lifetime of this instance to reduce the number of allocations.
    pub fn new<const N: usize>(init: [(&str, &str); N]) -> Self {
        // Get the executable path for the `MAKE` variable.
        let exe_path = match std::env::current_exe() {
            // TODO: This should probably be a `Result` instead of a `panic!`.
            // Try to canonicalize the path, but if that fails we panic because
            // this means something is very wrong with the system.
            Ok(path) => path.canonicalize().unwrap().to_string_lossy().into_owned(),
            Err(_) => panic!("Unable to get executable path!"),
        };

        // Rudimetary MAKEFLAGS parsing, the '-j' flag handling is not implemented yet.
        // TODO: This should probably be a `Result` instead of a `panic!`.
        // NOTE: Maybe change the way this is done to a pure IPC solution? when sub-make is used?
        let exe_args = std::env::args().collect::<Vec<_>>().iter().map(|arg| {
            let mut arg_mod = arg.clone();
            if arg_mod.contains(' ') {
                arg_mod = format!("\"{}\"", arg_mod);
            }
            if arg_mod.starts_with('-') {
                arg_mod = arg_mod[1..].to_string();
            }
            arg_mod
        }).collect::<Vec<_>>().join(" ");

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
            make_var: Var {
                value: exe_path,
                recursive: false,
            },
            makeargs_var: Var {
                value: exe_args,
                recursive: false,
            },
        };

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
            // Special variables.
            "MAKE" => &self.make_var,
            "MAKEFLAGS" => &self.makeargs_var,
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
            // Normal variables.
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

        // Variable names must not include whitespace or any chars in the set: `:#=`.
        for ch in k.chars() {
            if ch.is_whitespace() {
                return Err("Variable contains whitespace.".to_string());
            }

            if let Some(bad_char) = match ch {
                ':' => Some(':'),
                '#' => Some('#'),
                '=' => Some('='),
                _ => None,
            } {
                return Err(format!("Variable contains bad character '{}'.", bad_char));
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
