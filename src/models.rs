use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn is_zero(n: &usize) -> bool { *n == 0 }

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Java,
    Python,
    TypeScript,
    JavaScript,
    Go,
    Rust,
    CSharp,
    Cpp,
    Ruby,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Java => "java",
            Language::Python => "python",
            Language::TypeScript => "typescript",
            Language::JavaScript => "javascript",
            Language::Go => "go",
            Language::Rust => "rust",
            Language::CSharp => "csharp",
            Language::Cpp => "cpp",
            Language::Ruby => "ruby",
        }
    }
}

impl std::str::FromStr for Language {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "java" => Ok(Language::Java),
            "python" => Ok(Language::Python),
            "typescript" => Ok(Language::TypeScript),
            "javascript" => Ok(Language::JavaScript),
            "go" => Ok(Language::Go),
            "rust" => Ok(Language::Rust),
            "csharp" | "cs" => Ok(Language::CSharp),
            "cpp" | "c++" => Ok(Language::Cpp),
            "ruby" => Ok(Language::Ruby),
            other => Err(format!("unknown language: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub kind: String,      // class|interface|enum|method|constructor|field|function|struct|trait|type_alias
    pub name: String,
    #[serde(default)]
    pub modifiers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<Param>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub throws: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub implements: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub annotations: Vec<String>,
    pub line: usize,
    pub has_docstring: bool,
    #[serde(skip_serializing_if = "is_zero", default)]
    pub existing_word_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,                  // relative to project_root
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    pub comment_coverage: f64,
    pub skip: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
    pub cycle_warning: bool,
    pub internal_imports: Vec<String>, // resolved relative paths of internal deps
    pub signatures: Vec<Signature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scanned_at: Option<String>,   // per-entry rescan timestamp (RFC3339)
    /// Primary kind of the file's top-level declaration.
    /// Values: "class", "interface", "abstract_class", "annotation", "enum", "other"
    #[serde(default)]
    pub file_kind: String,
    /// Paths of files that implement, extend, or apply-as-annotation this file.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reverse_deps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub total_files: usize,
    pub skipped_covered: usize,
    pub skipped_generated: usize,
    pub to_comment: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheFile {
    pub version: u32,
    pub scanned_at: String,           // RFC3339
    pub project_root: String,
    pub detected_languages: Vec<String>,
    pub stats: Stats,
    #[serde(default)]
    pub min_docstring_words: usize,
    pub order: Vec<String>,           // relative paths in topo order (non-skipped only)
    pub files: HashMap<String, FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub project_root: String,
    pub detected_languages: Vec<String>,
    pub cache: String,
    pub stats: Stats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_roundtrip() {
        let sig = Signature {
            kind: "method".into(),
            name: "findByEmail".into(),
            modifiers: vec!["public".into(), "static".into()],
            params: Some(vec![Param { name: "email".into(), type_: "String".into() }]),
            return_type: Some("Optional<User>".into()),
            throws: vec!["DatabaseException".into()],
            extends: None,
            implements: vec![],
            annotations: vec![],
            line: 34,
            has_docstring: false,
            existing_word_count: 0,
            docstring_text: None,
        };
        let json = serde_json::to_string(&sig).unwrap();
        let back: Signature = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "findByEmail");
        assert_eq!(back.throws, vec!["DatabaseException"]);
    }

    #[test]
    fn language_serde_csharp() {
        let lang = Language::CSharp;
        let json = serde_json::to_string(&lang).unwrap();
        assert_eq!(json, "\"csharp\"");
        let back: Language = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Language::CSharp);
    }

    #[test]
    fn file_entry_skip_reason_omitted_when_none() {
        let entry = FileEntry {
            path: "src/Foo.java".into(),
            language: "java".into(),
            package: None,
            comment_coverage: 0.5,
            skip: false,
            skip_reason: None,
            cycle_warning: false,
            internal_imports: vec![],
            signatures: vec![],
            scanned_at: None,
            file_kind: "class".into(),
            reverse_deps: vec![],
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("skip_reason"), "skip_reason should be omitted when None, got: {}", json);
        assert!(!json.contains("package"), "package should be omitted when None");
        assert!(!json.contains("reverse_deps"), "reverse_deps should be omitted when empty, got: {}", json);
    }
}
