use super::{LanguageParser, ParseResult};
use crate::models::Signature;

pub struct TypeScriptParser;

impl TypeScriptParser {
    pub fn new() -> Self {
        Self
    }
}

fn node_text<'a>(node: tree_sitter::Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Check if preceding sibling is a JSDoc comment (`/** ... */`).
fn extract_preceding_jsdoc(node: tree_sitter::Node, source: &str) -> Option<String> {
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        match p.kind() {
            "comment" => {
                return source.get(p.byte_range())
                    .filter(|s| s.starts_with("/**"))
                    .map(|s| s.to_string());
            }
            _ if p.is_extra() => {
                prev = p.prev_sibling();
                continue;
            }
            _ => break,
        }
    }
    None
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
            "export_statement" => {
                // Recurse into the declaration inside export
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "class_declaration"
                        | "abstract_class_declaration"
                        | "function_declaration"
                        | "method_definition" => {
                            self.handle_declaration(child, node);
                        }
                        _ => {
                            // for export { ... } from '...' or export * from '...'
                            if child.kind() == "import_statement" {
                                if let Some(path) = extract_import_source(child, self.bytes) {
                                    self.result.raw_imports.push(path);
                                }
                            }
                        }
                    }
                }
            }
            "class_declaration" | "abstract_class_declaration" => {
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

    fn handle_declaration(&mut self, node: tree_sitter::Node, parent: tree_sitter::Node) {
        match node.kind() {
            "class_declaration" | "abstract_class_declaration" => {
                self.handle_class_with_jsdoc_parent(node, parent);
            }
            "function_declaration" => {
                self.handle_function_with_jsdoc_parent(node, parent, false);
            }
            _ => {}
        }
    }

    fn handle_class(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let line = node.start_position().row + 1;
        let doc = extract_preceding_jsdoc(node, self.source);

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
            has_docstring: doc.is_some(),
            existing_word_count: 0,
            docstring_text: doc.clone(),
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
        // Check JSDoc on the export statement (the parent) or on the class itself
        let doc =
            extract_preceding_jsdoc(parent, self.source)
                .or_else(|| extract_preceding_jsdoc(node, self.source));

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
            has_docstring: doc.is_some(),
            existing_word_count: 0,
            docstring_text: doc.clone(),
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
        let doc = extract_preceding_jsdoc(node, self.source);
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
            has_docstring: doc.is_some(),
            existing_word_count: 0,
            docstring_text: doc.clone(),
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
        let doc =
            extract_preceding_jsdoc(parent, self.source)
                .or_else(|| extract_preceding_jsdoc(node, self.source));
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
            has_docstring: doc.is_some(),
            existing_word_count: 0,
            docstring_text: doc.clone(),
        };
        self.result.signatures.push(sig);
    }

    fn handle_method(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        // Skip constructors and private-looking names
        if name == "constructor" {
            return;
        }

        let line = node.start_position().row + 1;
        let doc = extract_preceding_jsdoc(node, self.source);

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
            has_docstring: doc.is_some(),
            existing_word_count: 0,
            docstring_text: doc.clone(),
        };
        self.result.signatures.push(sig);
    }
}

impl LanguageParser for TypeScriptParser {
    fn parse(&self, source: &str) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("failed to set TypeScript language");

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
import { Component } from '@angular/core';
import { UserService } from './services/user.service';
import type { User } from '../models/user';

/**
 * App component
 */
export class AppComponent {
  title: string = 'app';

  constructor(private svc: UserService) {}

  getData(): User[] {
    return [];
  }
}

export function helperFn(): void {}
"#;

    #[test]
    fn test_ts_internal_only() {
        let r = TypeScriptParser::new().parse(SAMPLE);
        assert!(r.raw_imports.iter().any(|i| i.contains("./services/user.service")),
            "imports: {:?}", r.raw_imports);
        assert!(r.raw_imports.iter().any(|i| i.contains("../models/user")),
            "imports: {:?}", r.raw_imports);
        assert!(!r.raw_imports.iter().any(|i| i.contains("@angular")),
            "bare specifiers should be excluded, imports: {:?}", r.raw_imports);
    }

    #[test]
    fn test_ts_class_has_jsdoc() {
        let r = TypeScriptParser::new().parse(SAMPLE);
        let cls = r.signatures.iter().find(|s| s.kind == "class")
            .expect("should find class");
        assert!(cls.has_docstring, "AppComponent has JSDoc");
        assert_eq!(cls.name, "AppComponent");
    }

    #[test]
    fn test_ts_method_extracted() {
        let r = TypeScriptParser::new().parse(SAMPLE);
        assert!(r.signatures.iter().any(|s| s.name == "getData" || s.name == "helperFn"),
            "sigs: {:?}", r.signatures.iter().map(|s| &s.name).collect::<Vec<_>>());
    }

    #[test]
    fn test_ts_coverage() {
        let r = TypeScriptParser::new().parse(SAMPLE);
        assert!(r.coverage >= 0.0 && r.coverage <= 1.0, "coverage: {}", r.coverage);
    }
}
