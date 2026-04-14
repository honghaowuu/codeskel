use std::collections::HashMap;
use super::RefsAnalyzer;

pub struct JavaRefsAnalyzer;

impl JavaRefsAnalyzer {
    pub fn new() -> Self { Self }
}

impl RefsAnalyzer for JavaRefsAnalyzer {
    fn extract_refs(
        &self,
        _source: &str,
        _import_map: &HashMap<String, String>,
        _dep_sigs: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<String>> {
        HashMap::new() // stub — implemented in Task 5+6
    }
}
