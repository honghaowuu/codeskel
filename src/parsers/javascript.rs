use super::{LanguageParser, ParseResult};

pub struct JavaScriptParser;
impl JavaScriptParser { pub fn new() -> Self { Self } }
impl LanguageParser for JavaScriptParser {
    fn parse(&self, _source: &str) -> ParseResult {
        ParseResult::default()
    }
}
