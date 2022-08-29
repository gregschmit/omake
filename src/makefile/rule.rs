#[derive(Debug)]
pub struct Rule {
    pub targets: Vec<String>,
    pub dependencies: Vec<String>,
    pub recipe: Vec<String>,
    pub line: usize,
}
