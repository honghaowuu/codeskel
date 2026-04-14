pub mod java;

use std::collections::HashMap;
use crate::models::Language;

/// Analyzes a source file's body and returns, for each internal dep file,
/// the set of symbol names (from that dep's signatures) actually referenced.
pub trait RefsAnalyzer: Send + Sync {
    /// - `source`: raw source text of the file being analyzed
    /// - `import_map`: simple_name → dep_file_path (internal deps only)
    /// - `dep_sigs`: dep_file_path → list of signature names (all kinds)
    /// Returns: dep_file_path → sorted list of referenced symbol names
    fn extract_refs(
        &self,
        source: &str,
        import_map: &HashMap<String, String>,
        dep_sigs: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<String>>;
}

/// Returns a RefsAnalyzer for the given language, or None if unsupported.
pub fn get_refs_analyzer(lang: &Language) -> Option<Box<dyn RefsAnalyzer>> {
    match lang {
        Language::Java => Some(Box::new(java::JavaRefsAnalyzer::new())),
        _ => None,
    }
}
