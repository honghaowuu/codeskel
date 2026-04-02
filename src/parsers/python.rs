use super::{LanguageParser, ParseResult};

pub struct PythonParser;
impl PythonParser { pub fn new() -> Self { Self } }
impl LanguageParser for PythonParser {
    fn parse(&self, _source: &str) -> ParseResult {
        ParseResult::default()
    }
}
