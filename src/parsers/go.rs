use super::{LanguageParser, ParseResult};
use crate::models::Signature;

pub struct GoParser;

impl GoParser {
    pub fn new() -> Self {
        Self
    }
}

fn node_text<'a>(node: tree_sitter::Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

fn is_exported(name: &str) -> bool {
    name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
}

fn extract_preceding_comment(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut comments: Vec<String> = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "comment" {
            comments.push(p.utf8_text(bytes).unwrap_or("").to_string());
            prev = p.prev_sibling();
        } else if p.is_extra() {
            prev = p.prev_sibling();
        } else {
            break;
        }
    }
    if comments.is_empty() {
        None
    } else {
        comments.reverse();
        Some(comments.join("\n"))
    }
}

struct Walker<'a> {
    bytes: &'a [u8],
    result: &'a mut ParseResult,
}

impl<'a> Walker<'a> {
    fn walk_node(&mut self, node: tree_sitter::Node) {
        match node.kind() {
            "import_declaration" => {
                self.handle_import_declaration(node);
            }
            "type_declaration" => {
                self.handle_type_declaration(node);
            }
            "function_declaration" => {
                self.handle_function_declaration(node);
            }
            "method_declaration" => {
                self.handle_method_declaration(node);
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_node(child);
                }
            }
        }
    }

    fn handle_import_declaration(&mut self, node: tree_sitter::Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "import_spec" => {
                    self.extract_import_spec(child);
                }
                "import_spec_list" => {
                    let mut lc = child.walk();
                    for spec in child.children(&mut lc) {
                        if spec.kind() == "import_spec" {
                            self.extract_import_spec(spec);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn extract_import_spec(&mut self, node: tree_sitter::Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "interpreted_string_literal" || child.kind() == "raw_string_literal" {
                let text = node_text(child, self.bytes);
                let path = text.trim_matches('"').trim_matches('`');
                if !path.is_empty() {
                    self.result.raw_imports.push(path.to_string());
                }
                break;
            }
        }
    }

    fn handle_type_declaration(&mut self, node: tree_sitter::Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_spec" {
                self.handle_type_spec(child, node);
            }
        }
    }

    fn handle_type_spec(&mut self, spec_node: tree_sitter::Node, decl_node: tree_sitter::Node) {
        let name = match spec_node.child_by_field_name("name") {
            Some(n) => node_text(n, self.bytes).to_string(),
            None => return,
        };

        if !is_exported(&name) {
            return;
        }

        let line = spec_node.start_position().row + 1;

        // Check for doc comment on the parent type_declaration
        let doc = extract_preceding_comment(decl_node, self.bytes);

        let type_node = spec_node.child_by_field_name("type");
        let kind = match type_node.map(|n| n.kind()) {
            Some("struct_type") => "struct",
            Some("interface_type") => "interface",
            _ => "type",
        };

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
            docstring_text: doc.clone(),
        };
        self.result.signatures.push(sig);
    }

    fn handle_function_declaration(&mut self, node: tree_sitter::Node) {
        let name = match node.child_by_field_name("name") {
            Some(n) => node_text(n, self.bytes).to_string(),
            None => return,
        };

        if !is_exported(&name) {
            return;
        }

        let line = node.start_position().row + 1;
        let doc = extract_preceding_comment(node, self.bytes);

        let sig = Signature {
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
            docstring_text: doc.clone(),
        };
        self.result.signatures.push(sig);
    }

    fn handle_method_declaration(&mut self, node: tree_sitter::Node) {
        let name = match node.child_by_field_name("name") {
            Some(n) => node_text(n, self.bytes).to_string(),
            None => return,
        };

        if !is_exported(&name) {
            return;
        }

        let line = node.start_position().row + 1;
        let doc = extract_preceding_comment(node, self.bytes);

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
            docstring_text: doc.clone(),
        };
        self.result.signatures.push(sig);
    }
}

impl LanguageParser for GoParser {
    fn parse(&self, source: &str) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("failed to set Go language");

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

        // Coverage: documentable = exported types, exported functions, exported methods
        let documentable: Vec<&Signature> = result
            .signatures
            .iter()
            .filter(|s| matches!(s.kind.as_str(), "struct" | "interface" | "type" | "function" | "method"))
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
package main

import (
    "fmt"
    "github.com/myorg/myapp/models"
    "github.com/myorg/myapp/utils"
)

// UserService manages users.
type UserService struct {
    db *models.DB
}

// NewUserService creates a new service.
func NewUserService(db *models.DB) *UserService {
    return &UserService{db: db}
}

func (s *UserService) GetUser(id int) *models.User {
    return nil
}

func (s *UserService) privateHelper() {}
"#;

    #[test]
    fn test_go_imports() {
        let r = GoParser::new().parse(SAMPLE);
        assert!(r.raw_imports.iter().any(|i| i.contains("myapp/models")),
            "imports: {:?}", r.raw_imports);
        assert!(r.raw_imports.iter().any(|i| i.contains("myapp/utils")),
            "imports: {:?}", r.raw_imports);
        // fmt is included (resolver will filter it as stdlib)
        assert!(r.raw_imports.iter().any(|i| i == "fmt"),
            "stdlib imports should be included in raw: {:?}", r.raw_imports);
    }

    #[test]
    fn test_go_type_has_comment() {
        let r = GoParser::new().parse(SAMPLE);
        let s = r.signatures.iter().find(|s| s.name == "UserService")
            .expect("should find UserService");
        assert!(s.has_docstring, "UserService has line comment");
    }

    #[test]
    fn test_go_exported_func_only() {
        let r = GoParser::new().parse(SAMPLE);
        // NewUserService and GetUser are exported, privateHelper is not
        assert!(r.signatures.iter().any(|s| s.name == "NewUserService"),
            "sigs: {:?}", r.signatures.iter().map(|s| &s.name).collect::<Vec<_>>());
        assert!(r.signatures.iter().any(|s| s.name == "GetUser"),
            "method on receiver should be extracted");
        assert!(!r.signatures.iter().any(|s| s.name == "privateHelper"),
            "unexported methods must not appear");
    }

    #[test]
    fn test_go_coverage() {
        let r = GoParser::new().parse(SAMPLE);
        // UserService and NewUserService have comments; GetUser does not
        // total = 3 documentable, documented = 2
        assert!(r.coverage > 0.5 && r.coverage < 1.0,
            "coverage: {}", r.coverage);
    }
}
