pub mod java;
pub mod python;
pub mod typescript;
pub mod javascript;
pub mod go;
pub mod rust_lang;
pub mod csharp;
pub mod cpp;
pub mod ruby;

use crate::models::{Language, Signature};

/// Result of parsing a single source file.
#[derive(Debug, Default)]
pub struct ParseResult {
    /// Raw import strings as they appear in source (before resolution)
    pub raw_imports: Vec<String>,
    /// Extracted signatures (declarations only, no bodies)
    pub signatures: Vec<Signature>,
    /// (documented items) / (total documentable items)
    pub coverage: f64,
    /// Detected package/namespace (if applicable)
    pub package: Option<String>,
}

pub trait LanguageParser: Send + Sync {
    fn parse(&self, source: &str) -> ParseResult;
}

pub fn get_parser(lang: &Language) -> Box<dyn LanguageParser> {
    match lang {
        Language::Java => Box::new(java::JavaParser::new()),
        Language::Python => Box::new(python::PythonParser::new()),
        Language::TypeScript => Box::new(typescript::TypeScriptParser::new()),
        Language::JavaScript => Box::new(javascript::JavaScriptParser::new()),
        Language::Go => Box::new(go::GoParser::new()),
        Language::Rust => Box::new(rust_lang::RustParser::new()),
        Language::CSharp => Box::new(csharp::CSharpParser::new()),
        Language::Cpp => Box::new(cpp::CppParser::new()),
        Language::Ruby => Box::new(ruby::RubyParser::new()),
    }
}
