use super::{LanguageParser, ParseResult};
use crate::models::Signature;

pub struct CSharpParser;

impl CSharpParser {
    pub fn new() -> Self {
        Self
    }
}

fn node_text<'a>(node: tree_sitter::Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Check if a preceding sibling comment node starts with `///`.
fn extract_doc_comment(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut comments: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "comment" {
            let text = node_text(s, bytes);
            if text.starts_with("///") || text.starts_with("/**") {
                comments.push(text.to_string());
                sibling = s.prev_sibling();
                continue;
            }
            break;
        } else if s.is_extra() {
            sibling = s.prev_sibling();
            continue;
        } else {
            break;
        }
    }
    if comments.is_empty() {
        // Also check for /** block comment
        let mut sib = node.prev_sibling();
        while let Some(s) = sib {
            if s.kind() == "block_comment" {
                let text = node_text(s, bytes);
                if text.starts_with("/**") {
                    return Some(text.to_string());
                }
                return None;
            } else if s.is_extra() {
                sib = s.prev_sibling();
                continue;
            } else {
                break;
            }
        }
        None
    } else {
        comments.reverse();
        Some(comments.join("\n"))
    }
}

/// Check if a node has `public` or `protected` in its modifiers.
fn has_public_or_protected(node: tree_sitter::Node, bytes: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifier" {
            let text = node_text(child, bytes);
            if text == "public" || text == "protected" {
                return true;
            }
        }
    }
    false
}

struct Walker<'a> {
    bytes: &'a [u8],
    result: &'a mut ParseResult,
}

impl<'a> Walker<'a> {
    fn walk_node(&mut self, node: tree_sitter::Node) {
        match node.kind() {
            "using_directive" => {
                self.handle_using(node);
            }
            "class_declaration" => {
                self.handle_class_or_interface(node, "class");
            }
            "interface_declaration" => {
                self.handle_class_or_interface(node, "interface");
            }
            "namespace_declaration" => {
                self.handle_namespace(node);
            }
            // Also handle file-scoped namespace
            "file_scoped_namespace_declaration" => {
                self.handle_namespace(node);
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_node(child);
                }
            }
        }
    }

    fn handle_using(&mut self, node: tree_sitter::Node) {
        // using_directive has a name child with the namespace
        let full_text = node_text(node, self.bytes);
        // Strip "using " prefix and ";" suffix
        let ns = full_text
            .trim()
            .strip_prefix("using ")
            .unwrap_or("")
            .trim_end_matches(';')
            .trim();

        // Skip alias imports (contain '=')
        if ns.contains('=') {
            return;
        }

        // Strip 'static ' prefix
        let ns = ns.trim_start_matches("static ").trim();

        // Exclude System.* and Microsoft.*
        if ns.starts_with("System") || ns.starts_with("Microsoft") {
            return;
        }
        if !ns.is_empty() {
            self.result.raw_imports.push(ns.to_string());
        }
    }

    fn handle_namespace(&mut self, node: tree_sitter::Node) {
        // Recurse into the body of the namespace
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.walk_node(child);
            }
        } else {
            // file-scoped namespace: children at the same level
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "class_declaration" => self.handle_class_or_interface(child, "class"),
                    "interface_declaration" => self.handle_class_or_interface(child, "interface"),
                    _ => {}
                }
            }
        }
    }

    fn handle_class_or_interface(&mut self, node: tree_sitter::Node, kind: &str) {
        // Only public
        if !has_public_or_protected(node, self.bytes) {
            return;
        }

        let name = match node.child_by_field_name("name") {
            Some(n) => node_text(n, self.bytes).to_string(),
            None => return,
        };

        let doc = extract_doc_comment(node, self.bytes);
        let line = node.start_position().row + 1;

        self.result.signatures.push(Signature {
            kind: kind.to_string(),
            name,
            modifiers: vec!["public".to_string()],
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

        // Recurse into the body for methods
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "method_declaration" {
                    self.handle_method(child);
                } else if child.kind() == "constructor_declaration" {
                    self.handle_method(child);
                }
            }
        }
    }

    fn handle_method(&mut self, node: tree_sitter::Node) {
        if !has_public_or_protected(node, self.bytes) {
            return;
        }

        let name = match node.child_by_field_name("name") {
            Some(n) => node_text(n, self.bytes).to_string(),
            None => return,
        };

        let doc = extract_doc_comment(node, self.bytes);
        let line = node.start_position().row + 1;

        self.result.signatures.push(Signature {
            kind: "method".to_string(),
            name,
            modifiers: vec!["public".to_string()],
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
}

impl LanguageParser for CSharpParser {
    fn parse(&self, source: &str) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .expect("failed to set C# language");

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ParseResult::default(),
        };

        let bytes = source.as_bytes();

        // Uncomment to debug AST:
        // eprintln!("{}", tree.root_node().to_sexp());

        let mut result = ParseResult::default();

        let mut walker = Walker {
            bytes,
            result: &mut result,
        };
        walker.walk_node(tree.root_node());

        let total = result.signatures.len();
        let documented = result.signatures.iter().filter(|s| s.has_docstring).count();
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
using System;
using MyApp.Models;

namespace MyApp.Services
{
    /// <summary>
    /// User service.
    /// </summary>
    public class UserService
    {
        public User GetUser(int id) { return null; }
        private void Helper() {}
    }
}
"#;

    #[test]
    fn test_csharp_internal_import() {
        let r = CSharpParser::new().parse(SAMPLE);
        assert!(r.raw_imports.iter().any(|i| i == "MyApp.Models"),
            "imports: {:?}", r.raw_imports);
        assert!(!r.raw_imports.iter().any(|i| i == "System"),
            "System should be excluded: {:?}", r.raw_imports);
    }

    #[test]
    fn test_csharp_class_has_docstring() {
        let r = CSharpParser::new().parse(SAMPLE);
        let cls = r.signatures.iter().find(|s| s.kind == "class")
            .expect("should find class");
        assert!(cls.has_docstring);
    }

    #[test]
    fn test_csharp_public_method() {
        let r = CSharpParser::new().parse(SAMPLE);
        assert!(r.signatures.iter().any(|s| s.name == "GetUser"),
            "sigs: {:?}", r.signatures.iter().map(|s| &s.name).collect::<Vec<_>>());
        assert!(!r.signatures.iter().any(|s| s.name == "Helper"),
            "private methods must not appear");
    }

    #[test]
    fn test_csharp_using_static_excluded() {
        let src = "using static MyApp.Helpers.StringUtils;\nusing System;\npublic class Foo {}";
        let r = CSharpParser::new().parse(src);
        // "static MyApp.Helpers.StringUtils" should NOT appear (malformed key)
        // But "MyApp.Helpers.StringUtils" should appear after stripping "static "
        // And System should be excluded
        assert!(!r.raw_imports.iter().any(|i| i.starts_with("static ")),
            "static prefix must be stripped: {:?}", r.raw_imports);
        assert!(!r.raw_imports.iter().any(|i| i == "System"),
            "System must be excluded: {:?}", r.raw_imports);
    }
}
