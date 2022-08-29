mod expand;
mod var;

use std::io::BufRead;

use expand::expand;
use var::VarMap;

use super::MakeError;
use super::Rule;

const COMMENT_INDICATOR: char = '#';

/// This struct provides fields for the parser to keep internal state, and also provides a public
/// field to retrieve the parsed `rules` of the makefile.
///
/// I tried writing this in functional style, but most of my helper functions needed to know the
/// variable state, the current line number (for returning errors), and other stuff. Shared state
/// just seemed the easiest way to go.
pub struct Parser {
    pub rules: Vec<Rule>,

    vars: VarMap,
    current_rule: Option<Rule>,
    current_line_number: usize,
    // current_filename: String, // TODO: needed when doing include directives
}

/// The Parser is responsible for parsing makefiles (from a BufReader) intoa list of rules.
impl Parser {
    pub fn new<R: BufRead>(makefile_stream: R) -> Result<Self, MakeError> {
        let mut parser = Self {
            rules: vec![],
            vars: VarMap::new([]),
            current_rule: None,
            current_line_number: 0,
        };
        parser.parse(makefile_stream)?;
        Ok(parser)
    }

    /// The parser is responsible for extracting rules from the physical lines of the makefile
    /// stream, properly handling escaped newlines and semicolons, and also managing state, such as
    /// variable assignments.
    ///
    /// Escaped newlines outside of a recipe indicate a line continuation, whereas escaped newlines
    /// within a recipe are passed to the executing shell, as is. Semicolons after a rule definition
    /// should be interpreted as a newline plus a `.RECIPEPREFIX`.
    fn parse<R: BufRead>(&mut self, makefile_stream: R) -> Result<(), MakeError> {
        self.current_rule = None;

        for (i, result) in makefile_stream.lines().enumerate() {
            // Set the line number state and extract the line.
            self.current_line_number = i;
            let line = result.map_err(|e| MakeError::new(e.to_string(), i))?;

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
                        self.current_line_number,
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
                        r.recipe.push(expand(cmd, &self.vars)?);
                    }
                }
            }
            return Ok(());
        }

        // Anything other than recipe lines implicitly terminate a rule definition.
        if let Some(r) = self.current_rule.take() {
            self.rules.push(r);
        }

        // Ignore comments and blank lines.
        if line.starts_with(COMMENT_INDICATOR) || line.trim().is_empty() {
            return Ok(());
        }

        // Handle rule definitions.
        if let Some((targets, mut deps)) = line.split_once(':') {
            // There could be a semicolon after dependencies, in which case we should
            // parse everything after that as a rule line.
            let rule = deps.split_once(';').map(|(d, r)| {
                deps = d;
                r
            });

            self.current_rule = Some(Rule {
                targets: expand(targets, &self.vars)?
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                dependencies: expand(deps, &self.vars)?
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                recipe: vec![],
                line: self.current_line_number,
            });

            // Add rule line if we found one.
            if let Some(r) = rule {
                self.parse_line(format!("{}{}", self.vars.get(".RECIPEPREFIX").value, r))?;
            }

            return Ok(());
        }

        // Handle variable assignments.
        if let Some((k, v)) = line.split_once('=') {
            self.vars.set(k, v, false);
            return Ok(());
        }

        // Otherwise, throw error if line is not recognizable.
        return Err(MakeError::new(
            "Invalid line type.",
            self.current_line_number,
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
        let parser = Parser::new(input).unwrap();

        // Ensure we get exactly 1 rule.
        assert_eq!(parser.rules.len(), 1);

        // Ensure that rule has 1 target (all), no deps, and 1 recipe line.
        let rule = parser.rules.last().unwrap();
        assert_eq!(rule.targets.len(), 1);
        assert_eq!(rule.targets.first().unwrap(), "all");
        assert_eq!(rule.dependencies.len(), 0);
        assert_eq!(rule.recipe.len(), 1);
        assert_eq!(rule.recipe.first().unwrap(), "echo \"Hello, world!\"");
    }
}
