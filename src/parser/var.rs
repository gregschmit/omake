use std::collections::HashMap;

const DEFAULT_RECIPE_PREFIX: char = '\t';

pub struct Var {
    pub value: String,
    pub recursive: bool,
}

impl Var {
    pub fn new<S: Into<String>>(value: S, recursive: bool) -> Self {
        Self {
            value: value.into(),
            recursive: recursive,
        }
    }
}

/// This wraps a `HashMap` and a default value, providing an easy way to get variables, handling
/// special and automatic variables properly.
pub struct VarMap {
    pub map: HashMap<String, Var>,

    // Heap-allocated "constant" `Var` objects, setup during initialization, designed to reduce
    // multiple allocations and lifetime tracking.
    blank: Var,
    default_recipe_prefix: Var,
}

impl VarMap {
    /// Primary interface for configuring a new instance. We also create some cached values that
    /// live for the lifetime of this instance to reduce the number of allocations.
    pub fn new<const N: usize>(init: [(&str, &str); N]) -> Self {
        let map = HashMap::from(init.map(|e| {
            (
                e.0.to_string(),
                Var {
                    value: e.1.to_string(),
                    recursive: false,
                },
            )
        }));

        Self {
            map: map,
            blank: Var::new("", false),
            default_recipe_prefix: Var::new(DEFAULT_RECIPE_PREFIX, false),
        }
    }

    /// Public interface for getting variables.
    pub fn get<S: Into<String>>(&self, k: S) -> &Var {
        let k = k.into();
        match k.as_str() {
            ".RECIPEPREFIX" => match self.map.get(&k) {
                None => &self.default_recipe_prefix,
                Some(var) => {
                    if var.value.is_empty() {
                        &self.default_recipe_prefix
                    } else {
                        &var
                    }
                }
            },
            _ => match self.map.get(&k) {
                None => &self.blank,
                Some(var) => &var,
            },
        }
    }

    /// Public interface for setting variables.
    pub fn set<S: Into<String>>(&mut self, k: S, v: S, recursive: bool) {
        self.map.insert(k.into(), Var::new(v.into(), recursive));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_var_maps() {
        let vars = VarMap::new([("A", "B")]);
        assert_eq!(vars.get("A").value, "B");
        assert_eq!(vars.get("B").value, "");
    }

    #[test]
    fn test_recipe_prefix() {
        let mut vars = VarMap::new([]);
        assert_eq!(vars.get(".RECIPEPREFIX").value, "\t");
        vars.set(".RECIPEPREFIX", "B", false);
        assert_eq!(vars.get(".RECIPEPREFIX").value, "B");
        vars.set(".RECIPEPREFIX", "", false);
        assert_eq!(vars.get(".RECIPEPREFIX").value, "\t");
    }
}
