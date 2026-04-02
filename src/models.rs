use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
            Language::CSharp => "cs",
            Language::Cpp => "cpp",
            Language::Ruby => "ruby",
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,                  // relative to project_root
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    pub comment_coverage: f64,
    pub skip: bool,
    pub skip_reason: Option<String>,
    pub cycle_warning: bool,
    pub internal_imports: Vec<String>, // resolved relative paths of internal deps
    pub signatures: Vec<Signature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scanned_at: Option<String>,   // per-entry rescan timestamp (RFC3339)
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
        };
        let json = serde_json::to_string(&sig).unwrap();
        let back: Signature = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "findByEmail");
        assert_eq!(back.throws, vec!["DatabaseException"]);
    }
}
