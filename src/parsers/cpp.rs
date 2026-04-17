use super::{LanguageParser, ParseResult};
use crate::models::Signature;

pub struct CppParser;

impl CppParser {
    pub fn new() -> Self {
        Self
    }
}

fn node_text<'a>(node: tree_sitter::Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Check if preceding sibling comment starts with `/**` or `/*!`.
fn extract_doc_comment(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "comment" {
            let text = node_text(s, bytes);
            if text.starts_with("/**") || text.starts_with("/*!") {
                return Some(text.to_string());
            }
            return None;
        } else if s.is_extra() {
            sibling = s.prev_sibling();
            continue;
        } else {
            break;
        }
    }
    None
}

/// Walk declarator chain to find the function name identifier.
fn find_function_name(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    match node.kind() {
        "function_declarator" => {
            let decl = node.child_by_field_name("declarator")?;
            find_function_name(decl, bytes)
        }
        "identifier" | "qualified_identifier" | "destructor_name" | "operator_name" => {
            Some(node_text(node, bytes).to_string())
        }
        "pointer_declarator" | "reference_declarator" => {
            let inner = node.child_by_field_name("declarator")?;
            find_function_name(inner, bytes)
        }
        _ => None,
    }
}

fn extract_function_name(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let declarator = node.child_by_field_name("declarator")?;
    find_function_name(declarator, bytes)
}

struct Walker<'a> {
    bytes: &'a [u8],
    result: &'a mut ParseResult,
}

impl<'a> Walker<'a> {
    fn walk_node(&mut self, node: tree_sitter::Node) {
        match node.kind() {
            "preproc_include" => {
                self.handle_include(node);
            }
            "function_definition" => {
                self.handle_function(node);
            }
            "class_specifier" => {
                self.handle_class(node);
            }
            "struct_specifier" => {
                self.handle_struct(node);
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_node(child);
                }
            }
        }
    }

    fn handle_include(&mut self, node: tree_sitter::Node) {
        // Look for path child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "string_literal" => {
                    // Quoted include: strip the outer quotes
                    let text = node_text(child, self.bytes);
                    let stripped = text.trim_matches('"');
                    if !stripped.is_empty() {
                        self.result.raw_imports.push(stripped.to_string());
                    }
                }
                "system_lib_string" => {
                    // Angle-bracket include: skip
                }
                _ => {}
            }
        }
    }

    fn handle_function(&mut self, node: tree_sitter::Node) {
        let name = match extract_function_name(node, self.bytes) {
            Some(n) => n,
            None => return,
        };

        let doc = extract_doc_comment(node, self.bytes);
        let line = node.start_position().row + 1;

        self.result.signatures.push(Signature {
            kind: "function".to_string(),
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
        });
    }

    fn handle_class(&mut self, node: tree_sitter::Node) {
        let name = match node.child_by_field_name("name") {
            Some(n) => node_text(n, self.bytes).to_string(),
            None => return,
        };

        let doc = extract_doc_comment(node, self.bytes);
        let line = node.start_position().row + 1;

        self.result.signatures.push(Signature {
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
        });
    }

    fn handle_struct(&mut self, node: tree_sitter::Node) {
        let name = match node.child_by_field_name("name") {
            Some(n) => node_text(n, self.bytes).to_string(),
            None => return,
        };

        let doc = extract_doc_comment(node, self.bytes);
        let line = node.start_position().row + 1;

        self.result.signatures.push(Signature {
            kind: "struct".to_string(),
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
        });
    }
}

impl LanguageParser for CppParser {
    fn parse(&self, source: &str) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .expect("failed to set C++ language");

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
#include <vector>
#include "mylib/foo.h"
#include "utils.h"

/**
 * Main function.
 */
void process(int x) {}

class MyClass {
public:
    void doWork() {}
};
"#;

    #[test]
    fn test_cpp_quoted_include() {
        let r = CppParser::new().parse(SAMPLE);
        assert!(r.raw_imports.iter().any(|i| i == "mylib/foo.h"),
            "imports: {:?}", r.raw_imports);
        assert!(r.raw_imports.iter().any(|i| i == "utils.h"),
            "imports: {:?}", r.raw_imports);
        assert!(!r.raw_imports.iter().any(|i| i == "vector"),
            "angle-bracket includes must be excluded: {:?}", r.raw_imports);
    }

    #[test]
    fn test_cpp_function_has_jsdoc() {
        let r = CppParser::new().parse(SAMPLE);
        let f = r.signatures.iter().find(|s| s.name == "process");
        assert!(f.is_some(), "process should be extracted, sigs: {:?}",
            r.signatures.iter().map(|s| &s.name).collect::<Vec<_>>());
        assert!(f.unwrap().has_docstring, "process has /** */ before it");
    }

    #[test]
    fn test_cpp_class_extracted() {
        let r = CppParser::new().parse(SAMPLE);
        assert!(r.signatures.iter().any(|s| s.kind == "class" && s.name == "MyClass"),
            "sigs: {:?}", r.signatures.iter().map(|s| (&s.kind, &s.name)).collect::<Vec<_>>());
    }
}
