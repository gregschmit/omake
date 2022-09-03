use super::Context;

#[derive(Debug)]
pub struct Rule {
    pub targets: Vec<String>,
    pub dependencies: Vec<String>,
    pub recipe: Vec<String>,
    pub context: Context,
}
