use super::{LanguageParser, ParseResult};

pub struct RubyParser;
impl RubyParser { pub fn new() -> Self { Self } }
impl LanguageParser for RubyParser {
    fn parse(&self, _source: &str) -> ParseResult {
        ParseResult::default()
    }
}
