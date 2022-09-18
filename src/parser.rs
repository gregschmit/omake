mod expand;
mod var;

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use expand::expand;
use var::VarMap;

use super::{log_warn, Context, MakeError, Rule, RuleMap};

const COMMENT_INDICATOR: char = '#';

/// This struct provides fields for the parser to keep internal state, and also provides a public
/// field to retrieve the parsed `rulemap` of the makefile.
pub struct Parser {
    pub rulemap: RuleMap,
    pub default_target: Option<String>,

    vars: VarMap,
    current_rule: Option<Rule>,
    current_context: Context,
}

/// The Parser is responsible for parsing makefiles into a map of rules.
impl Parser {
    /// Initialize and run parser from a file path.
    pub fn from_file(makefile_fn: PathBuf) -> Result<Self, MakeError> {
        // Open the makefile.
        let file = File::open(&makefile_fn).map_err(|e| {
            MakeError::new(format!("Could not read makefile ({}).", e), Context::new())
        })?;

        // Initialize the parser.
        let mut parser = Self {
            rulemap: RuleMap::new(),
            default_target: None,
            vars: VarMap::new([]),
            current_rule: None,
            current_context: Context::from_path(makefile_fn),
        };

        // Parse the makefile.
        parser.parse(BufReader::new(file))?;

        Ok(parser)
    }

    /// Initialize and run parser from a stream.
    #[allow(dead_code)]
    pub fn from_stream<R: BufRead>(stream: R) -> Result<Self, MakeError> {
        // Initialize the parser.
        let mut parser = Self {
            rulemap: RuleMap::new(),
            default_target: None,
            vars: VarMap::new([]),
            current_rule: None,
            current_context: Context::new(),
        };

        // Parse the stream.
        parser.parse(stream)?;

        Ok(parser)
    }

    /// The parser is responsible for extracting rules from the physical lines of the makefile
    /// stream, properly handling escaped newlines and semicolons, and also managing state, such as
    /// variable assignments.
    ///
    /// Escaped newlines outside of a recipe indicate a line continuation, whereas escaped newlines
    /// within a recipe are passed to the executing shell, as is. Semicolons after a rule definition
    /// should be interpreted as a newline plus a `.RECIPEPREFIX`.
    fn parse<R: BufRead>(&mut self, stream: R) -> Result<(), MakeError> {
        self.current_rule = None;

        for (i, result) in stream.lines().enumerate() {
            // Set the context line number and extract the line.
            self.current_context.line_number = i + 1;
            let line =
                result.map_err(|e| MakeError::new(e.to_string(), self.current_context.clone()))?;

            // Parse the line.
            self.parse_line(line)?;
        }

        // Always push a blank line at the end to terminate trailing rules.
        self.parse_line("".to_string())?;

        Ok(())
    }

    /// The line parser is responsible for detecting line type and handling them appropriately.
    fn parse_line(&mut self, line: String) -> Result<(), MakeError> {
        // Handle recipe lines.
        let recipe_prefix = &self.vars.get(".RECIPEPREFIX").value;
        if line.starts_with(recipe_prefix) {
            // If line starts with the recipe prefix, then push it to the current rule.
            match &mut self.current_rule {
                None => {
                    return Err(MakeError::new(
                        "recipe without rule",
                        self.current_context.clone(),
                    ))
                }
                Some(r) => {
                    // Strip the recipe prefix first.
                    let cmd = line
                        .strip_prefix(recipe_prefix)
                        .expect("line known to start with a recipe prefix")
                        .trim()
                        .to_string();

                    if !cmd.is_empty() {
                        r.recipe
                            .push(expand(cmd.as_str(), &self.vars, &self.current_context)?);
                    }
                }
            }
            return Ok(());
        }

        // Anything other than recipe lines terminate a rule definition.
        if let Some(rule) = self.current_rule.take() {
            for target in rule.targets.iter() {
                // Set default target if none is specified and this is a normal target. Note that
                // `unwrap()` here is safe because the target is a result of splitting on
                // whitespace, which would result in an empty array if there is only whitespace or
                // no text.
                if self.default_target.is_none() && target.chars().nth(0).unwrap() != '.' {
                    self.default_target = Some(target.clone());
                }

                // Finihs terminating this rule definition (adding to rulemap).
                match self.rulemap.get_mut(target) {
                    Some(existing_rules) => {
                        // Catch user mixing single and double-colon rules.
                        if let Some(first) = existing_rules.first() {
                            if first.double_colon != rule.double_colon {
                                return Err(MakeError::new(
                                    "Cannot define rules using `:` and `::` on the same target.",
                                    self.current_context.clone(),
                                ));
                            }
                        }

                        if rule.double_colon {
                            existing_rules.push(rule.clone())
                        } else {
                            log_warn(
                                "Ignoring duplicate definition.",
                                Some(&self.current_context),
                            );
                        }
                    }
                    None => {
                        self.rulemap.insert(target.to_owned(), vec![rule.clone()]);
                    }
                }
            }
        }

        // Ignore comments and blank lines.
        if line.starts_with(COMMENT_INDICATOR) || line.trim().is_empty() {
            return Ok(());
        }

        // Handle rule definitions.
        if let Some((targets, mut deps)) = line.split_once(':') {
            // First, if deps start with another `:`, then this is a double-colon rule, so we should
            // mark it as such.
            let mut double_colon = false;
            if let Some(ch) = deps.chars().next() {
                if ch == ':' {
                    deps = &deps[1..];
                    double_colon = true;
                }
            }

            // There could be a semicolon after dependencies, in which case we should
            // parse everything after that as a rule line.
            let rule = deps.split_once(';').map(|(d, r)| {
                deps = d;
                r
            });

            self.current_rule = Some(Rule {
                targets: expand(targets, &self.vars, &self.current_context)?
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                dependencies: expand(deps, &self.vars, &self.current_context)?
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                recipe: vec![],
                context: self.current_context.clone(),
                double_colon: double_colon,
            });

            // Add rule line if we found one.
            if let Some(r) = rule {
                self.parse_line(format!("{}{}", self.vars.get(".RECIPEPREFIX").value, r))?;
            }

            return Ok(());
        }

        // Handle variable assignments.
        if let Some((k, v)) = line.split_once('=') {
            if let Err(e) = self.vars.set(
                k,
                &expand(v.trim_start(), &self.vars, &self.current_context)?,
                false,
            ) {
                return Err(MakeError::new(e, self.current_context.clone()));
            };
            return Ok(());
        }

        // Otherwise, throw error if line is not recognizable.
        return Err(MakeError::new(
            "Invalid line type.",
            self.current_context.clone(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn test_simple() {
        let input = BufReader::new("all: ;echo \"Hello, world!\"".as_bytes());
        let parser = Parser::from_stream(input).unwrap();

        // Ensure we get exactly 1 target (all) in rulemap.
        let rulemap = parser.rulemap;
        assert_eq!(rulemap.len(), 1);

        // Ensure we have 1 rule for that target.
        let rules = rulemap.values().next().unwrap();
        assert_eq!(rules.len(), 1);

        // Ensure that rule has no deps and 1 recipe line.
        let rule = rules.first().unwrap();
        assert_eq!(rule.dependencies.len(), 0);
        assert_eq!(rule.recipe.len(), 1);
        assert_eq!(rule.recipe.first().unwrap(), "echo \"Hello, world!\"");
    }
}
