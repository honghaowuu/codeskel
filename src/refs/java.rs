use std::collections::HashMap;
use tree_sitter::Node;
use super::RefsAnalyzer;

pub struct JavaRefsAnalyzer;

impl JavaRefsAnalyzer {
    pub fn new() -> Self { Self }
}

fn node_text<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Extract simple type name from a type node (handles generic_type wrapper).
fn type_name_from_node(node: Node, bytes: &[u8]) -> String {
    match node.kind() {
        "type_identifier" => node_text(node, bytes).to_string(),
        "generic_type" => {
            let mut cursor = node.walk();
            let result = node.children(&mut cursor)
                .find(|c| c.kind() == "type_identifier")
                .map(|c| node_text(c, bytes).to_string())
                .unwrap_or_default();
            result
        }
        _ => String::new(),
    }
}

/// Phase 1: Walk the AST and build a map of `var_name → declared_type_simple_name`
/// by inspecting local_variable_declaration, field_declaration, and formal_parameter nodes.
pub fn build_type_map(node: Node, bytes: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    collect_type_map(node, bytes, &mut map);
    map
}

fn collect_type_map(node: Node, bytes: &[u8], map: &mut HashMap<String, String>) {
    match node.kind() {
        "local_variable_declaration" | "field_declaration" => {
            if let Some(type_node) = node.child_by_field_name("type") {
                let t = type_name_from_node(type_node, bytes);
                if !t.is_empty() {
                    // Each variable_declarator child holds the variable name
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "variable_declarator" {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let var = node_text(name_node, bytes).to_string();
                                if !var.is_empty() {
                                    map.insert(var, t.clone());
                                }
                            }
                        }
                    }
                }
            }
            // Recurse for nested declarations (e.g. inside initializer lambdas)
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_type_map(child, bytes, map);
            }
        }
        "formal_parameter" => {
            if let Some(type_node) = node.child_by_field_name("type") {
                let t = type_name_from_node(type_node, bytes);
                if !t.is_empty() {
                    // In tree-sitter-java, formal_parameter has a direct `name` field (identifier)
                    // and optionally a `name` inside `variable_declarator_id` for array params.
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let var = node_text(name_node, bytes).to_string();
                        if !var.is_empty() {
                            map.insert(var, t.clone());
                        }
                    } else {
                        // Fallback: look for variable_declarator_id child
                        let mut cursor = node.walk();
                        for child in node.children(&mut cursor) {
                            if child.kind() == "variable_declarator_id" {
                                if let Some(name_node) = child.child_by_field_name("name") {
                                    let var = node_text(name_node, bytes).to_string();
                                    if !var.is_empty() {
                                        map.insert(var, t.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_type_map(child, bytes, map);
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_type_map(child, bytes, map);
            }
        }
    }
}

impl RefsAnalyzer for JavaRefsAnalyzer {
    fn extract_refs(
        &self,
        _source: &str,
        _import_map: &HashMap<String, String>,
        _dep_sigs: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<String>> {
        HashMap::new() // stub — Phase 2+3 in Task 6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_java(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn test_build_type_map_local_var() {
        let source = r#"
class Foo {
    void method() {
        User user = null;
        UserRepository repo;
    }
}
"#;
        let tree = parse_java(source);
        let bytes = source.as_bytes();
        let map = build_type_map(tree.root_node(), bytes);
        assert_eq!(map.get("user").map(String::as_str), Some("User"),
            "user should map to User; map: {:?}", map);
        assert_eq!(map.get("repo").map(String::as_str), Some("UserRepository"),
            "repo should map to UserRepository; map: {:?}", map);
    }

    #[test]
    fn test_build_type_map_field_decl() {
        let source = r#"
class Foo {
    private UserRepository repo;
    public User currentUser;
}
"#;
        let tree = parse_java(source);
        let bytes = source.as_bytes();
        let map = build_type_map(tree.root_node(), bytes);
        assert_eq!(map.get("repo").map(String::as_str), Some("UserRepository"));
        assert_eq!(map.get("currentUser").map(String::as_str), Some("User"));
    }

    #[test]
    fn test_build_type_map_formal_param() {
        let source = r#"
class Foo {
    void save(User user, String name) {}
}
"#;
        let tree = parse_java(source);
        let bytes = source.as_bytes();
        let map = build_type_map(tree.root_node(), bytes);
        assert_eq!(map.get("user").map(String::as_str), Some("User"));
        assert_eq!(map.get("name").map(String::as_str), Some("String"));
    }
}
