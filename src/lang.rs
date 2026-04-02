use crate::models::Language;
use std::path::Path;

pub fn detect_language(path: &Path) -> Option<Language> {
    match path.extension()?.to_str()? {
        "java" => Some(Language::Java),
        "py" => Some(Language::Python),
        "ts" | "tsx" => Some(Language::TypeScript),
        "js" | "jsx" | "mjs" => Some(Language::JavaScript),
        "go" => Some(Language::Go),
        "rs" => Some(Language::Rust),
        "cs" => Some(Language::CSharp),
        "c" | "cpp" | "h" | "hpp" => Some(Language::Cpp),
        "rb" => Some(Language::Ruby),
        _ => None,
    }
}

/// Parse a language from a CLI --lang argument.
/// Accepts both short forms (ts, js, cs) and full names (typescript, javascript, csharp).
pub fn lang_from_str(s: &str) -> Option<Language> {
    match s {
        "java" => Some(Language::Java),
        "python" => Some(Language::Python),
        "ts" | "typescript" => Some(Language::TypeScript),
        "js" | "javascript" => Some(Language::JavaScript),
        "go" => Some(Language::Go),
        "rust" => Some(Language::Rust),
        "cs" | "csharp" => Some(Language::CSharp),
        "cpp" => Some(Language::Cpp),
        "ruby" => Some(Language::Ruby),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_java() { assert_eq!(detect_language(Path::new("Foo.java")), Some(Language::Java)); }
    #[test]
    fn test_python() { assert_eq!(detect_language(Path::new("foo.py")), Some(Language::Python)); }
    #[test]
    fn test_tsx() { assert_eq!(detect_language(Path::new("App.tsx")), Some(Language::TypeScript)); }
    #[test]
    fn test_rs_extension() { assert_eq!(detect_language(Path::new("main.rs")), Some(Language::Rust)); }
    #[test]
    fn test_unknown() { assert_eq!(detect_language(Path::new("README.md")), None); }
    #[test]
    fn test_lang_from_str_cs() { assert_eq!(lang_from_str("cs"), Some(Language::CSharp)); }
}
