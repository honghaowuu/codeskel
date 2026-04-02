use super::{LanguageParser, ParseResult};

pub struct TypeScriptParser;
impl TypeScriptParser { pub fn new() -> Self { Self } }
impl LanguageParser for TypeScriptParser {
    fn parse(&self, _source: &str) -> ParseResult {
        ParseResult::default()
    }
}
