use super::{LanguageParser, ParseResult};

pub struct JavaParser;
impl JavaParser { pub fn new() -> Self { Self } }
impl LanguageParser for JavaParser {
    fn parse(&self, _source: &str) -> ParseResult {
        ParseResult::default()
    }
}
