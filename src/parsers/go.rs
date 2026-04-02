use super::{LanguageParser, ParseResult};

pub struct GoParser;
impl GoParser { pub fn new() -> Self { Self } }
impl LanguageParser for GoParser {
    fn parse(&self, _source: &str) -> ParseResult {
        ParseResult::default()
    }
}
