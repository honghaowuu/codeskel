use super::{LanguageParser, ParseResult};
use crate::models::Signature;

pub struct JavaScriptParser;

impl JavaScriptParser {
    pub fn new() -> Self {
        Self
    }
}

fn node_text<'a>(node: tree_sitter::Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Check if preceding sibling is a JSDoc comment (`/** ... */`).
fn preceding_jsdoc(node: tree_sitter::Node, source: &str) -> bool {
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        match p.kind() {
            "comment" => {
                return source[p.byte_range()].starts_with("/**");
            }
            _ if p.is_extra() => {
                prev = p.prev_sibling();
                continue;
            }
            _ => break,
        }
    }
    false
}

/// Extract the import source path from an import_statement node.
/// Returns only relative paths (starting with `./` or `../`).
fn extract_import_source(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let src = node.child_by_field_name("source")?;
    let raw = src.utf8_text(bytes).ok()?;
    let path = raw.trim_matches('"').trim_matches('\'').trim_matches('`');
    if path.starts_with("./") || path.starts_with("../") {
        Some(path.to_string())
    } else {
        None
    }
}

struct Walker<'a> {
    bytes: &'a [u8],
    source: &'a str,
    result: &'a mut ParseResult,
}

impl<'a> Walker<'a> {
    fn walk_node(&mut self, node: tree_sitter::Node) {
        match node.kind() {
            "import_statement" => {
                if let Some(path) = extract_import_source(node, self.bytes) {
                    self.result.raw_imports.push(path);
                }
            }
            "call_expression" => {
                self.handle_require(node);
                // Still recurse into children (e.g., chained calls)
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_node(child);
                }
            }
            "export_statement" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "class_declaration" => {
                            self.handle_class_with_jsdoc_parent(child, node);
                        }
                        "function_declaration" => {
                            self.handle_function_with_jsdoc_parent(child, node, false);
                        }
                        _ => {}
                    }
                }
            }
            "class_declaration" => {
                self.handle_class(node);
            }
            "function_declaration" => {
                self.handle_function(node, false);
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_node(child);
                }
            }
        }
    }

    fn handle_require(&mut self, node: tree_sitter::Node) {
        let fn_node = match node.child_by_field_name("function") {
            Some(n) => n,
            None => return,
        };
        if node_text(fn_node, self.bytes) != "require" {
            return;
        }
        if let Some(args) = node.child_by_field_name("arguments") {
            let mut cursor = args.walk();
            for arg in args.children(&mut cursor) {
                if arg.kind() == "string" {
                    let raw = node_text(arg, self.bytes);
                    let path = raw.trim_matches('"').trim_matches('\'').trim_matches('`');
                    if path.starts_with("./") || path.starts_with("../") {
                        self.result.raw_imports.push(path.to_string());
                    }
                }
            }
        }
    }

    fn handle_class(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let line = node.start_position().row + 1;
        let has_doc = preceding_jsdoc(node, self.source);

        let sig = Signature {
            kind: "class".to_string(),
            name,
            modifiers: Vec::new(),
            params: None,
            return_type: None,
            throws: Vec::new(),
            extends: None,
            implements: Vec::new(),
            annotations: Vec::new(),
            line,
            has_docstring: has_doc,
        };
        self.result.signatures.push(sig);

        // Recurse into class body for methods
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "method_definition" {
                    self.handle_method(child);
                }
            }
        }
    }

    fn handle_class_with_jsdoc_parent(
        &mut self,
        node: tree_sitter::Node,
        parent: tree_sitter::Node,
    ) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let line = node.start_position().row + 1;
        let has_doc =
            preceding_jsdoc(parent, self.source) || preceding_jsdoc(node, self.source);

        let sig = Signature {
            kind: "class".to_string(),
            name,
            modifiers: Vec::new(),
            params: None,
            return_type: None,
            throws: Vec::new(),
            extends: None,
            implements: Vec::new(),
            annotations: Vec::new(),
            line,
            has_docstring: has_doc,
        };
        self.result.signatures.push(sig);

        // Recurse into class body for methods
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "method_definition" {
                    self.handle_method(child);
                }
            }
        }
    }

    fn handle_function(&mut self, node: tree_sitter::Node, is_method: bool) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let line = node.start_position().row + 1;
        let has_doc = preceding_jsdoc(node, self.source);
        let kind = if is_method { "method" } else { "function" };

        let sig = Signature {
            kind: kind.to_string(),
            name,
            modifiers: Vec::new(),
            params: None,
            return_type: None,
            throws: Vec::new(),
            extends: None,
            implements: Vec::new(),
            annotations: Vec::new(),
            line,
            has_docstring: has_doc,
        };
        self.result.signatures.push(sig);
    }

    fn handle_function_with_jsdoc_parent(
        &mut self,
        node: tree_sitter::Node,
        parent: tree_sitter::Node,
        is_method: bool,
    ) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let line = node.start_position().row + 1;
        let has_doc =
            preceding_jsdoc(parent, self.source) || preceding_jsdoc(node, self.source);
        let kind = if is_method { "method" } else { "function" };

        let sig = Signature {
            kind: kind.to_string(),
            name,
            modifiers: Vec::new(),
            params: None,
            return_type: None,
            throws: Vec::new(),
            extends: None,
            implements: Vec::new(),
            annotations: Vec::new(),
            line,
            has_docstring: has_doc,
        };
        self.result.signatures.push(sig);
    }

    fn handle_method(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        // Skip constructors
        if name == "constructor" {
            return;
        }

        let line = node.start_position().row + 1;
        let has_doc = preceding_jsdoc(node, self.source);

        let sig = Signature {
            kind: "method".to_string(),
            name,
            modifiers: Vec::new(),
            params: None,
            return_type: None,
            throws: Vec::new(),
            extends: None,
            implements: Vec::new(),
            annotations: Vec::new(),
            line,
            has_docstring: has_doc,
        };
        self.result.signatures.push(sig);
    }
}

impl LanguageParser for JavaScriptParser {
    fn parse(&self, source: &str) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .expect("failed to set JavaScript language");

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ParseResult::default(),
        };

        let bytes = source.as_bytes();
        let mut result = ParseResult::default();

        let mut walker = Walker {
            bytes,
            source,
            result: &mut result,
        };
        walker.walk_node(tree.root_node());

        // Coverage: documentable = classes, functions, methods
        let documentable: Vec<&Signature> = result
            .signatures
            .iter()
            .filter(|s| matches!(s.kind.as_str(), "class" | "function" | "method"))
            .collect();

        let documented = documentable.iter().filter(|s| s.has_docstring).count();
        let total = documentable.len();
        result.coverage = if total > 0 {
            documented as f64 / total as f64
        } else {
            1.0
        };

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
const path = require('path');
const utils = require('./utils');

/**
 * Main class
 */
class MyClass {
  doWork() {}
}

function helper() {}
"#;

    #[test]
    fn test_js_require_relative() {
        let r = JavaScriptParser::new().parse(SAMPLE);
        assert!(r.raw_imports.iter().any(|i| i.contains("./utils")),
            "imports: {:?}", r.raw_imports);
        assert!(!r.raw_imports.iter().any(|i| i == "path"),
            "bare requires should be excluded, imports: {:?}", r.raw_imports);
    }

    #[test]
    fn test_js_class_jsdoc() {
        let r = JavaScriptParser::new().parse(SAMPLE);
        let cls = r.signatures.iter().find(|s| s.kind == "class");
        assert!(cls.is_some(), "should find class");
        assert!(cls.unwrap().has_docstring);
    }
}
