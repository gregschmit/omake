//! A wrapper for a `HashMap` for storing the environment variables during makefile.
//!
//! The only interesting behavior here is that for some special keys we have default values which
//! should be "resettable" by setting the value to blank, and that calling `get` on a key that
//! doesn't exist should return an empty `Var`. To support these behaviors without polluting the
//! underlying `HashMap` with lots of duplicate data, the `Vars` struct contains fields for those
//! heap-allocated "constant" objects. Since we always return a reference to a `Var`, this is quite
//! efficient.

use std::collections::HashMap;

const DEFAULT_RECIPE_PREFIX: char = '\t';

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

        // Use `set` to initialize data.
        for (k, v) in init {
            let _ = vars.set(k, v, false);
        }

        vars
    }

    /// Public interface for getting variables. For unknown keys, the `blank` object is returned,
    /// and some special keys have default values.
    pub fn get<S: Into<String>>(&self, k: S) -> &Var {
        let k = k.into();
        match k.as_str() {
            ".RECIPEPREFIX" => match self.map.get(&k) {
                None => &self.default_recipe_prefix,
                Some(var) => {
                    if var.value.is_empty() {
                        &self.default_recipe_prefix
                    } else {
                        var
                    }
                }
            },
            _ => match self.map.get(&k) {
                None => &self.blank,
                Some(var) => var,
            },
        }
    }

    /// Public interface for setting variables.
    pub fn set<S: Into<String>>(&mut self, k: S, v: S, recursive: bool) -> Result<(), String> {
        let clean_key = k.into().trim().to_string();

        // Variable names must not include whitespace or any chars in the set: `:#=`.
        for ch in clean_key.chars() {
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
            clean_key,
            Var {
                value: v.into(),
                recursive,
            },
        );
        Ok(())
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
