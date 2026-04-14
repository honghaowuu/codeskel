use std::collections::{HashMap, HashSet};
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

/// Phase 2: Walk the AST collecting (type_name, Option<member_name>) candidate pairs.
/// Skips import_declaration and package_declaration subtrees entirely.
fn collect_candidates(
    node: Node,
    bytes: &[u8],
    type_map: &HashMap<String, String>,
    out: &mut Vec<(String, Option<String>)>,
) {
    match node.kind() {
        // Skip — identifiers here are not project type references
        "import_declaration" | "package_declaration" => {}

        "type_identifier" => {
            let name = node_text(node, bytes).to_string();
            if !name.is_empty() {
                out.push((name, None));
            }
            // type_identifier is a leaf; no children to recurse
        }

        "object_creation_expression" => {
            // new User(...) — extract the constructor type
            if let Some(t) = node.child_by_field_name("type") {
                let name = type_name_from_node(t, bytes);
                if !name.is_empty() {
                    out.push((name, None));
                }
            }
            // Recurse into all children (arguments, etc.).
            // Note: the type child also triggers the type_identifier arm, producing a
            // duplicate (name, None) entry. Phase 3 deduplicates via HashSet — harmless.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_candidates(child, bytes, type_map, out);
            }
        }

        "method_invocation" => {
            // Resolve receiver → determine which dep type owns this method call
            if let Some(obj) = node.child_by_field_name("object") {
                let obj_text = node_text(obj, bytes);
                let resolved = if let Some(t) = type_map.get(obj_text) {
                    Some(t.clone()) // var → declared type
                } else if obj_text.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    Some(obj_text.to_string()) // ClassName.staticMethod(...)
                } else {
                    None // chained call result or unknown — discard
                };
                if let Some(recv_type) = resolved {
                    // When receiver is an uppercase identifier (static/class ref), also
                    // emit a type-level candidate so the class itself is cross-referenced.
                    if obj_text.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                        && type_map.get(obj_text).is_none()
                    {
                        out.push((recv_type.clone(), None));
                    }
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let method = node_text(name_node, bytes).to_string();
                        if !method.is_empty() {
                            out.push((recv_type, Some(method)));
                        }
                    }
                }
            }
            // Recurse into all children (arguments, and any nested invocations)
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_candidates(child, bytes, type_map, out);
            }
        }

        "field_access" => {
            if let Some(obj) = node.child_by_field_name("object") {
                let obj_text = node_text(obj, bytes);
                let resolved = if let Some(t) = type_map.get(obj_text) {
                    Some(t.clone())
                } else if obj_text.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    Some(obj_text.to_string())
                } else {
                    None
                };
                if let Some(recv_type) = resolved {
                    if let Some(field_node) = node.child_by_field_name("field") {
                        let field = node_text(field_node, bytes).to_string();
                        if !field.is_empty() {
                            out.push((recv_type, Some(field)));
                        }
                    }
                }
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_candidates(child, bytes, type_map, out);
            }
        }

        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_candidates(child, bytes, type_map, out);
            }
        }
    }
}

impl RefsAnalyzer for JavaRefsAnalyzer {
    fn extract_refs(
        &self,
        source: &str,
        import_map: &HashMap<String, String>,
        dep_sigs: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<String>> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return HashMap::new(),
        };
        let bytes = source.as_bytes();
        let root = tree.root_node();

        // Phase 1
        let type_map = build_type_map(root, bytes);

        // Phase 2
        let mut candidates: Vec<(String, Option<String>)> = Vec::new();
        collect_candidates(root, bytes, &type_map, &mut candidates);

        // Phase 3: cross-reference against dep signatures, dedup via HashSet
        let mut refs: HashMap<String, HashSet<String>> = HashMap::new();
        for (type_name, member_opt) in candidates {
            if let Some(dep_path) = import_map.get(&type_name) {
                if let Some(sigs) = dep_sigs.get(dep_path) {
                    let name_to_check = member_opt.as_deref().unwrap_or(&type_name);
                    if sigs.iter().any(|s| s == name_to_check) {
                        refs.entry(dep_path.clone())
                            .or_default()
                            .insert(name_to_check.to_string());
                    }
                }
            }
        }

        // Convert to sorted Vec for deterministic output
        refs.into_iter()
            .map(|(k, v)| {
                let mut names: Vec<String> = v.into_iter().collect();
                names.sort();
                (k, names)
            })
            .collect()
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

    #[test]
    fn test_extract_refs_userservice() {
        let source = r#"
package com.example.service;
import com.example.model.User;
import com.example.repo.UserRepository;

public class UserService {
    private UserRepository repo;

    public User findUser(String id) {
        User user = repo.findById(id);
        return user;
    }

    public void saveUser(User user) {
        repo.save(user);
    }

    public String getUserEmail(String id) {
        User user = repo.findById(id);
        return user.getEmail();
    }
}
"#;
        let mut import_map = HashMap::new();
        import_map.insert("User".to_string(),
            "src/com/example/model/User.java".to_string());
        import_map.insert("UserRepository".to_string(),
            "src/com/example/repo/UserRepository.java".to_string());

        let mut dep_sigs: HashMap<String, Vec<String>> = HashMap::new();
        dep_sigs.insert("src/com/example/model/User.java".to_string(),
            vec!["User".to_string(), "getEmail".to_string(), "setEmail".to_string()]);
        dep_sigs.insert("src/com/example/repo/UserRepository.java".to_string(),
            vec!["UserRepository".to_string(), "findById".to_string(), "save".to_string()]);

        let analyzer = JavaRefsAnalyzer::new();
        let refs = analyzer.extract_refs(source, &import_map, &dep_sigs);

        let user_refs = refs.get("src/com/example/model/User.java")
            .expect("User.java must appear in refs");
        assert!(user_refs.contains(&"User".to_string()),
            "User type ref missing; got: {:?}", user_refs);
        assert!(user_refs.contains(&"getEmail".to_string()),
            "getEmail method ref missing; got: {:?}", user_refs);

        let repo_refs = refs.get("src/com/example/repo/UserRepository.java")
            .expect("UserRepository.java must appear in refs");
        assert!(repo_refs.contains(&"UserRepository".to_string()),
            "UserRepository type ref missing; got: {:?}", repo_refs);
        assert!(repo_refs.contains(&"findById".to_string()),
            "findById missing; got: {:?}", repo_refs);
        assert!(repo_refs.contains(&"save".to_string()),
            "save missing; got: {:?}", repo_refs);
    }

    #[test]
    fn test_extract_refs_no_internal_imports() {
        let source = "public class Foo { void bar() { String s = \"x\"; } }";
        let import_map: HashMap<String, String> = HashMap::new();
        let dep_sigs: HashMap<String, Vec<String>> = HashMap::new();

        let analyzer = JavaRefsAnalyzer::new();
        let refs = analyzer.extract_refs(source, &import_map, &dep_sigs);
        assert!(refs.is_empty(), "no internal imports → empty refs");
    }

    #[test]
    fn test_extract_refs_static_call() {
        let source = r#"
import com.example.Util;
class Foo {
    void bar() {
        Util.doSomething();
    }
}
"#;
        let mut import_map = HashMap::new();
        import_map.insert("Util".to_string(), "src/Util.java".to_string());
        let mut dep_sigs: HashMap<String, Vec<String>> = HashMap::new();
        dep_sigs.insert("src/Util.java".to_string(),
            vec!["Util".to_string(), "doSomething".to_string()]);

        let analyzer = JavaRefsAnalyzer::new();
        let refs = analyzer.extract_refs(source, &import_map, &dep_sigs);
        let util_refs = refs.get("src/Util.java").expect("Util.java must appear");
        assert!(util_refs.contains(&"Util".to_string()),
            "Util type_identifier ref missing; got: {:?}", util_refs);
        assert!(util_refs.contains(&"doSomething".to_string()),
            "static method call missing; got: {:?}", util_refs);
    }
}
