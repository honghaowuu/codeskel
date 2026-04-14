use super::{LanguageParser, ParseResult};
use crate::models::Signature;

pub struct RustParser;

impl RustParser {
    pub fn new() -> Self {
        Self
    }
}

fn node_text<'a>(node: tree_sitter::Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Check if the node has a preceding `///` line comment or `/**` block comment.
fn extract_doc_comment(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut comments: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" => {
                let text = node_text(s, bytes);
                if text.starts_with("///") {
                    comments.push(text.to_string());
                    sibling = s.prev_sibling();
                    continue;
                }
                break;
            }
            "block_comment" => {
                let text = node_text(s, bytes);
                if text.starts_with("/**") {
                    return Some(text.to_string());
                }
                return None;
            }
            _ if s.is_extra() => {
                sibling = s.prev_sibling();
                continue;
            }
            _ => break,
        }
    }
    if comments.is_empty() {
        None
    } else {
        comments.reverse();
        Some(comments.join("\n"))
    }
}

/// Check if a node has `pub` visibility by looking for visibility_modifier child.
fn is_pub(node: tree_sitter::Node, bytes: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            let text = node_text(child, bytes);
            return text.starts_with("pub");
        }
    }
    false
}

/// Find the first `type_identifier` or `identifier` child (for struct/trait/enum names).
fn find_name_child<'a>(node: tree_sitter::Node, bytes: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_identifier" || child.kind() == "identifier" {
            return Some(node_text(child, bytes));
        }
    }
    None
}

struct Walker<'a> {
    bytes: &'a [u8],
    result: &'a mut ParseResult,
}

impl<'a> Walker<'a> {
    fn walk_node(&mut self, node: tree_sitter::Node) {
        match node.kind() {
            "use_declaration" => {
                self.handle_use(node);
            }
            "function_item" => {
                self.handle_function(node);
            }
            "struct_item" => {
                self.handle_named_item(node, "struct");
            }
            "trait_item" => {
                self.handle_named_item(node, "trait");
            }
            "enum_item" => {
                self.handle_named_item(node, "enum");
            }
            "impl_item" => {
                self.handle_impl(node);
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_node(child);
                }
            }
        }
    }

    fn handle_use(&mut self, node: tree_sitter::Node) {
        // Get the full text of the use declaration
        // e.g. `use crate::models::User;`  → `crate::models::User`
        let full_text = node_text(node, self.bytes);
        let trimmed = full_text
            .trim()
            .strip_prefix("use ")
            .unwrap_or(full_text)
            .trim_end_matches(';')
            .trim();

        // Only keep crate:: or super:: imports
        if trimmed.starts_with("crate::") || trimmed.starts_with("super::") {
            self.result.raw_imports.push(trimmed.to_string());
        }
    }

    fn handle_function(&mut self, node: tree_sitter::Node) {
        if !is_pub(node, self.bytes) {
            return;
        }

        let name = match find_name_child(node, self.bytes) {
            Some(n) => n.to_string(),
            None => return,
        };

        let doc = extract_doc_comment(node, self.bytes);
        let line = node.start_position().row + 1;

        self.result.signatures.push(Signature {
            kind: "function".to_string(),
            name,
            modifiers: vec!["pub".to_string()],
            params: None,
            return_type: None,
            throws: Vec::new(),
            extends: None,
            implements: Vec::new(),
            annotations: Vec::new(),
            line,
            has_docstring: doc.is_some(),
            docstring_text: doc.clone(),
        });
    }

    fn handle_named_item(&mut self, node: tree_sitter::Node, kind: &str) {
        if !is_pub(node, self.bytes) {
            return;
        }

        let name = match find_name_child(node, self.bytes) {
            Some(n) => n.to_string(),
            None => return,
        };

        let doc = extract_doc_comment(node, self.bytes);
        let line = node.start_position().row + 1;

        self.result.signatures.push(Signature {
            kind: kind.to_string(),
            name,
            modifiers: vec!["pub".to_string()],
            params: None,
            return_type: None,
            throws: Vec::new(),
            extends: None,
            implements: Vec::new(),
            annotations: Vec::new(),
            line,
            has_docstring: doc.is_some(),
            docstring_text: doc.clone(),
        });
    }

    fn handle_impl(&mut self, node: tree_sitter::Node) {
        // Recurse into impl body to find function_item nodes
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "function_item" {
                    self.handle_function(child);
                }
            }
        } else {
            // Fallback: find declaration_list child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "declaration_list" {
                    let mut inner = child.walk();
                    for inner_child in child.children(&mut inner) {
                        if inner_child.kind() == "function_item" {
                            self.handle_function(inner_child);
                        }
                    }
                    break;
                }
            }
        }
    }
}

impl LanguageParser for RustParser {
    fn parse(&self, source: &str) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("failed to set Rust language");

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ParseResult::default(),
        };

        let bytes = source.as_bytes();

        // Debug: uncomment to see the AST
        // eprintln!("{}", tree.root_node().to_sexp());

        let mut result = ParseResult::default();

        let mut walker = Walker {
            bytes,
            result: &mut result,
        };
        walker.walk_node(tree.root_node());

        // Coverage: pub items only
        let documentable: Vec<&Signature> = result.signatures.iter().collect();
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
use crate::models::User;
use std::collections::HashMap;

/// A user service.
pub struct UserService {
    db: String,
}

impl UserService {
    /// Creates a new service.
    pub fn new(db: String) -> Self {
        UserService { db }
    }

    fn private_method(&self) {}
}

pub trait Processor {
    fn process(&self);
}

pub enum Status { Active, Inactive }
"#;

    #[test]
    fn test_rust_crate_import() {
        let r = RustParser::new().parse(SAMPLE);
        assert!(r.raw_imports.iter().any(|i| i == "crate::models::User"),
            "imports: {:?}", r.raw_imports);
        assert!(!r.raw_imports.iter().any(|i| i.starts_with("std::")),
            "std imports should be excluded: {:?}", r.raw_imports);
    }

    #[test]
    fn test_rust_pub_struct() {
        let r = RustParser::new().parse(SAMPLE);
        let s = r.signatures.iter().find(|s| s.name == "UserService")
            .expect("should find UserService");
        assert_eq!(s.kind, "struct");
        assert!(s.has_docstring);
    }

    #[test]
    fn test_rust_pub_method_in_impl() {
        let r = RustParser::new().parse(SAMPLE);
        assert!(r.signatures.iter().any(|s| s.name == "new" && s.kind == "function"),
            "pub fn new inside impl should be extracted");
        assert!(!r.signatures.iter().any(|s| s.name == "private_method"),
            "private methods must not appear");
    }

    #[test]
    fn test_rust_trait_and_enum() {
        let r = RustParser::new().parse(SAMPLE);
        assert!(r.signatures.iter().any(|s| s.kind == "trait" && s.name == "Processor"));
        assert!(r.signatures.iter().any(|s| s.kind == "enum" && s.name == "Status"));
    }
}
