use super::{LanguageParser, ParseResult};
use crate::models::{Param, Signature};

pub struct JavaParser;
impl JavaParser {
    pub fn new() -> Self {
        Self
    }
}

fn node_text<'a>(node: tree_sitter::Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Return the text of the preceding Javadoc block comment, or `None`.
fn extract_javadoc(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "block_comment" {
            let text = node_text(s, bytes);
            if text.starts_with("/**") {
                return Some(text.to_string());
            }
            return None;
        } else if s.kind() == "line_comment" || s.is_extra() {
            sibling = s.prev_sibling();
            continue;
        } else {
            break;
        }
    }
    None
}

/// Extract modifiers from a modifiers node or from direct children
fn extract_modifiers_from_node(modifiers_node: tree_sitter::Node, bytes: &[u8]) -> Vec<String> {
    let mut mods = Vec::new();
    let mut cursor = modifiers_node.walk();
    for child in modifiers_node.children(&mut cursor) {
        match child.kind() {
            "public" | "private" | "protected" | "static" | "final" | "abstract"
            | "synchronized" | "native" | "transient" | "volatile" | "strictfp" => {
                mods.push(node_text(child, bytes).to_string());
            }
            _ => {}
        }
    }
    mods
}

fn extract_modifiers(node: tree_sitter::Node, bytes: &[u8]) -> Vec<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            return extract_modifiers_from_node(child, bytes);
        }
    }
    Vec::new()
}

fn has_modifier(mods: &[String], modifier: &str) -> bool {
    mods.iter().any(|m| m == modifier)
}

/// Extract parameters from a formal_parameters node
fn extract_params(params_node: tree_sitter::Node, bytes: &[u8]) -> Vec<Param> {
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        if child.kind() == "formal_parameter" || child.kind() == "spread_parameter" {
            let mut param_cursor = child.walk();
            let mut param_type = String::new();
            let mut param_name = String::new();
            for pchild in child.children(&mut param_cursor) {
                match pchild.kind() {
                    "type_identifier" | "generic_type" | "array_type" | "integral_type"
                    | "floating_point_type" | "boolean_type" | "void_type"
                    | "scoped_type_identifier" => {
                        param_type = node_text(pchild, bytes).to_string();
                    }
                    "variable_declarator_id" => {
                        // The name is the identifier inside
                        if let Some(id) = pchild.child_by_field_name("name") {
                            param_name = node_text(id, bytes).to_string();
                        } else {
                            param_name = node_text(pchild, bytes).to_string();
                        }
                    }
                    "identifier" => {
                        param_name = node_text(pchild, bytes).to_string();
                    }
                    _ => {}
                }
            }
            if !param_name.is_empty() || !param_type.is_empty() {
                params.push(Param {
                    name: param_name,
                    type_: param_type,
                });
            }
        }
    }
    params
}

fn extract_throws(node: tree_sitter::Node, bytes: &[u8]) -> Vec<String> {
    let mut throws = vec![];
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "throws" {
            let mut c2 = child.walk();
            for t in child.children(&mut c2) {
                if t.kind() == "type_identifier" || t.kind() == "scoped_type_identifier" {
                    let text = t.utf8_text(bytes).unwrap_or("").to_string();
                    if !text.is_empty() {
                        throws.push(text);
                    }
                }
            }
        }
    }
    throws
}

struct Walker<'a> {
    bytes: &'a [u8],
    result: &'a mut ParseResult,
}

impl<'a> Walker<'a> {
    fn walk_node(&mut self, node: tree_sitter::Node) {
        match node.kind() {
            "package_declaration" => {
                self.handle_package(node);
            }
            "import_declaration" => {
                self.handle_import(node);
            }
            "class_declaration" => {
                self.handle_class(node);
            }
            "interface_declaration" => {
                self.handle_interface(node);
            }
            "enum_declaration" => {
                self.handle_enum(node);
            }
            "method_declaration" => {
                self.handle_method(node);
            }
            "constructor_declaration" => {
                self.handle_constructor(node);
            }
            "field_declaration" => {
                self.handle_field(node);
            }
            _ => {
                // Recurse into children
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_node(child);
                }
            }
        }
    }

    fn handle_package(&mut self, node: tree_sitter::Node) {
        // Walk children to find scoped_identifier or identifier
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "scoped_identifier" | "identifier" => {
                    self.result.package = Some(node_text(child, self.bytes).to_string());
                    return;
                }
                _ => {}
            }
        }
    }

    fn handle_import(&mut self, node: tree_sitter::Node) {
        // Walk children to find scoped_identifier or identifier
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "scoped_identifier" | "identifier" => {
                    self.result
                        .raw_imports
                        .push(node_text(child, self.bytes).to_string());
                    return;
                }
                _ => {}
            }
        }
    }

    fn handle_class(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let modifiers = extract_modifiers(node, self.bytes);
        let doc = extract_javadoc(node, self.bytes);
        let line = node.start_position().row + 1;

        // Find extends (superclass)
        let mut extends: Option<String> = None;
        let mut implements: Vec<String> = Vec::new();

        // Walk children for superclass and super_interfaces
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "superclass" => {
                    // child is "superclass" node, contains "type_identifier"
                    let mut sc_cursor = child.walk();
                    for sc_child in child.children(&mut sc_cursor) {
                        if sc_child.kind() == "type_identifier"
                            || sc_child.kind() == "generic_type"
                        {
                            extends = Some(node_text(sc_child, self.bytes).to_string());
                            break;
                        }
                    }
                }
                "super_interfaces" => {
                    let mut si_cursor = child.walk();
                    for si_child in child.children(&mut si_cursor) {
                        if si_child.kind() == "type_list" || si_child.kind() == "interface_type_list" {
                            let mut tl_cursor = si_child.walk();
                            for tl_child in si_child.children(&mut tl_cursor) {
                                if tl_child.kind() == "type_identifier"
                                    || tl_child.kind() == "generic_type"
                                {
                                    implements
                                        .push(node_text(tl_child, self.bytes).to_string());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let sig = Signature {
            kind: "class".to_string(),
            name,
            modifiers,
            params: None,
            return_type: None,
            throws: Vec::new(),
            extends,
            implements,
            annotations: Vec::new(),
            line,
            has_docstring: doc.is_some(),
            existing_word_count: 0,
            docstring_text: doc.clone(),
        };
        self.result.signatures.push(sig);

        // Recurse into body
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.walk_node(child);
            }
        }
    }

    fn handle_interface(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let modifiers = extract_modifiers(node, self.bytes);
        let doc = extract_javadoc(node, self.bytes);
        let line = node.start_position().row + 1;

        let sig = Signature {
            kind: "interface".to_string(),
            name,
            modifiers,
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

        // Recurse into body
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.walk_node(child);
            }
        }
    }

    fn handle_enum(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let modifiers = extract_modifiers(node, self.bytes);
        let doc = extract_javadoc(node, self.bytes);
        let line = node.start_position().row + 1;

        let sig = Signature {
            kind: "enum".to_string(),
            name,
            modifiers,
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

        // Recurse into body
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.walk_node(child);
            }
        }
    }

    fn handle_method(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let return_type = node
            .child_by_field_name("type")
            .map(|n| node_text(n, self.bytes).to_string());

        let params = node
            .child_by_field_name("parameters")
            .map(|n| extract_params(n, self.bytes))
            .unwrap_or_default();

        let modifiers = extract_modifiers(node, self.bytes);
        let doc = extract_javadoc(node, self.bytes);
        let line = node.start_position().row + 1;

        let sig = Signature {
            kind: "method".to_string(),
            name,
            modifiers,
            params: Some(params),
            return_type,
            throws: extract_throws(node, self.bytes),
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

    fn handle_constructor(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let params = node
            .child_by_field_name("parameters")
            .map(|n| extract_params(n, self.bytes))
            .unwrap_or_default();

        let modifiers = extract_modifiers(node, self.bytes);
        let doc = extract_javadoc(node, self.bytes);
        let line = node.start_position().row + 1;

        let sig = Signature {
            kind: "constructor".to_string(),
            name,
            modifiers,
            params: Some(params),
            return_type: None,
            throws: extract_throws(node, self.bytes),
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

    fn handle_field(&mut self, node: tree_sitter::Node) {
        let modifiers = extract_modifiers(node, self.bytes);
        // Only extract public or protected fields
        if !has_modifier(&modifiers, "public") && !has_modifier(&modifiers, "protected") {
            return;
        }

        let field_type = node
            .child_by_field_name("type")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let doc = extract_javadoc(node, self.bytes);
        let line = node.start_position().row + 1;

        // Find variable_declarator children for names
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                let name = child
                    .child_by_field_name("name")
                    .map(|n| node_text(n, self.bytes).to_string())
                    .unwrap_or_default();

                if name.is_empty() {
                    continue;
                }

                let sig = Signature {
                    kind: "field".to_string(),
                    name,
                    modifiers: modifiers.clone(),
                    params: None,
                    return_type: Some(field_type.clone()),
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
    }
}

impl LanguageParser for JavaParser {
    fn parse(&self, source: &str) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("failed to set Java language");

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

        // Calculate coverage: documented / total documentable items
        let documentable: Vec<&Signature> = result
            .signatures
            .iter()
            .filter(|s| matches!(s.kind.as_str(), "class" | "interface" | "enum" | "method" | "constructor" | "field"))
            .collect();

        let documented = documentable.iter().filter(|s| s.has_docstring).count();
        let total = documentable.len();
        result.coverage = if total > 0 { documented as f64 / total as f64 } else { 1.0 };

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
package com.example.model;

import java.util.Optional;
import com.example.base.Entity;

/**
 * Represents a user.
 */
public class User extends Entity implements Auditable {
    private Long id;
    public String name;

    public User() {}

    public Optional<User> findByEmail(String email) {
        return Optional.empty();
    }
}
"#;

    #[test]
    fn test_java_imports() {
        let r = JavaParser::new().parse(SAMPLE);
        assert!(
            r.raw_imports.contains(&"java.util.Optional".to_string()),
            "imports: {:?}",
            r.raw_imports
        );
        assert!(
            r.raw_imports.contains(&"com.example.base.Entity".to_string()),
            "imports: {:?}",
            r.raw_imports
        );
    }

    #[test]
    fn test_java_class_signature() {
        let r = JavaParser::new().parse(SAMPLE);
        let cls = r
            .signatures
            .iter()
            .find(|s| s.kind == "class")
            .expect("should find class");
        assert_eq!(cls.name, "User");
        assert_eq!(cls.extends.as_deref(), Some("Entity"));
        assert!(cls.has_docstring, "User class has /** javadoc */");
    }

    #[test]
    fn test_java_method_signature() {
        let r = JavaParser::new().parse(SAMPLE);
        let m = r
            .signatures
            .iter()
            .find(|s| s.name == "findByEmail")
            .expect("should find findByEmail");
        assert_eq!(m.kind, "method");
        assert!(!m.has_docstring);
        // Generic return types must be captured correctly
        assert_eq!(m.return_type.as_deref(), Some("Optional<User>"),
            "return_type: {:?}", m.return_type);
    }

    #[test]
    fn test_java_package() {
        let r = JavaParser::new().parse(SAMPLE);
        assert_eq!(
            r.package.as_deref(),
            Some("com.example.model"),
            "package: {:?}",
            r.package
        );
    }

    #[test]
    fn test_java_coverage() {
        let r = JavaParser::new().parse(SAMPLE);
        // class has docstring (1 out of several documentable items)
        assert!(
            r.coverage > 0.0 && r.coverage <= 1.0,
            "coverage: {}",
            r.coverage
        );
    }

    #[test]
    fn test_java_class_docstring_text_extracted() {
        let r = JavaParser::new().parse(SAMPLE);
        let cls = r.signatures.iter().find(|s| s.kind == "class").unwrap();
        let text = cls.docstring_text.as_deref().expect("should have docstring_text");
        assert!(text.contains("Represents a user"), "text: {:?}", text);
    }

    #[test]
    fn test_java_method_without_doc_has_no_docstring_text() {
        let r = JavaParser::new().parse(SAMPLE);
        let m = r.signatures.iter().find(|s| s.name == "findByEmail").unwrap();
        assert!(m.docstring_text.is_none(), "should be None, got: {:?}", m.docstring_text);
    }

    #[test]
    fn test_java_public_field() {
        let r = JavaParser::new().parse(SAMPLE);
        // public String name should be extracted
        assert!(
            r.signatures
                .iter()
                .any(|s| s.kind == "field" && s.name == "name"),
            "sigs: {:?}",
            r.signatures
                .iter()
                .map(|s| (&s.kind, &s.name))
                .collect::<Vec<_>>()
        );
    }
}
