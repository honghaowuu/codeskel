use super::{LanguageParser, ParseResult};

pub struct RustParser;
impl RustParser { pub fn new() -> Self { Self } }
impl LanguageParser for RustParser {
    fn parse(&self, _source: &str) -> ParseResult {
        ParseResult::default()
    }
}
