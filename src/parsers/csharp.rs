use super::{LanguageParser, ParseResult};

pub struct CSharpParser;
impl CSharpParser { pub fn new() -> Self { Self } }
impl LanguageParser for CSharpParser {
    fn parse(&self, _source: &str) -> ParseResult {
        ParseResult::default()
    }
}
