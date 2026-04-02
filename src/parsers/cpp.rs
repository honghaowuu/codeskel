use super::{LanguageParser, ParseResult};

pub struct CppParser;
impl CppParser { pub fn new() -> Self { Self } }
impl LanguageParser for CppParser {
    fn parse(&self, _source: &str) -> ParseResult {
        ParseResult::default()
    }
}
