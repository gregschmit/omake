use std::collections::HashMap;

use super::Context;

/// Represents a parsed rule from a makefile.
#[derive(Debug, Clone)]
pub struct Rule {
    pub targets: Vec<String>,
    pub dependencies: Vec<String>,
    pub recipe: Vec<String>,
    pub context: Context,
    pub double_colon: bool,
}

/// Mapping of individual targets to vectors of rules which reference the target.
pub type RuleMap = HashMap<String, Vec<Rule>>;
