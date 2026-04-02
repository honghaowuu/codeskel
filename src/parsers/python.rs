use super::{LanguageParser, ParseResult};
use crate::models::{Param, Signature};

pub struct PythonParser;

impl PythonParser {
    pub fn new() -> Self {
        Self
    }
}

fn node_text<'a>(node: tree_sitter::Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Check if the first body statement is a docstring (expression_statement containing a string).
fn first_body_is_docstring(body: tree_sitter::Node, _bytes: &[u8]) -> bool {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "expression_statement" => {
                if let Some(inner) = child.child(0) {
                    return inner.kind() == "string";
                }
                return false;
            }
            "comment" | "\n" => continue,
            _ => return false,
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
            "import_statement" => {
                self.handle_import(node);
            }
            "import_from_statement" => {
                self.handle_import_from(node);
            }
            "class_definition" => {
                self.handle_class(node);
            }
            "function_definition" => {
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

    /// `import X` or `import X, Y`
    fn handle_import(&mut self, node: tree_sitter::Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "dotted_name" | "identifier" => {
                    self.result
                        .raw_imports
                        .push(node_text(child, self.bytes).to_string());
                }
                // aliased_import: `import X as Y` — grab the dotted_name inside
                "aliased_import" => {
                    let mut ac = child.walk();
                    for ac_child in child.children(&mut ac) {
                        if ac_child.kind() == "dotted_name" || ac_child.kind() == "identifier" {
                            self.result
                                .raw_imports
                                .push(node_text(ac_child, self.bytes).to_string());
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// `from X import Y` — record X (the module path)
    fn handle_import_from(&mut self, node: tree_sitter::Node) {
        if let Some(module) = node.child_by_field_name("module_name") {
            self.result
                .raw_imports
                .push(node_text(module, self.bytes).to_string());
        }
    }

    fn handle_class(&mut self, node: tree_sitter::Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        let line = node.start_position().row + 1;

        let has_doc = node
            .child_by_field_name("body")
            .map(|body| first_body_is_docstring(body, self.bytes))
            .unwrap_or(false);

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

        // Recurse into body for methods
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "function_definition" {
                    self.handle_function(child, true);
                }
                // Nested classes
                if child.kind() == "class_definition" {
                    self.handle_class(child);
                }
            }
        }
    }

    fn handle_function(&mut self, node: tree_sitter::Node, is_method: bool) {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, self.bytes).to_string())
            .unwrap_or_default();

        // Skip private: starts with `_` (includes __init__, _private, etc.)
        if name.starts_with('_') {
            return;
        }

        let line = node.start_position().row + 1;

        let has_doc = node
            .child_by_field_name("body")
            .map(|body| first_body_is_docstring(body, self.bytes))
            .unwrap_or(false);

        let return_type = node
            .child_by_field_name("return_type")
            .map(|n| node_text(n, self.bytes).to_string());

        // Extract parameters
        let params = node
            .child_by_field_name("parameters")
            .map(|params_node| extract_params(params_node, self.bytes))
            .unwrap_or_default();

        let kind = if is_method { "method" } else { "function" };

        let sig = Signature {
            kind: kind.to_string(),
            name,
            modifiers: Vec::new(),
            params: Some(params),
            return_type,
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

fn extract_params(params_node: tree_sitter::Node, bytes: &[u8]) -> Vec<Param> {
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                let name = node_text(child, bytes).to_string();
                if name != "self" && name != "cls" {
                    params.push(Param { name, type_: String::new() });
                }
            }
            "typed_parameter" => {
                // `name: type`
                let param_name = child
                    .child_by_field_name("name")
                    .map(|n| node_text(n, bytes).to_string())
                    .unwrap_or_default();
                let param_type = child
                    .child_by_field_name("type")
                    .map(|n| node_text(n, bytes).to_string())
                    .unwrap_or_default();
                if !param_name.is_empty() && param_name != "self" && param_name != "cls" {
                    params.push(Param { name: param_name, type_: param_type });
                }
            }
            "default_parameter" | "typed_default_parameter" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let param_name = node_text(name_node, bytes).to_string();
                    let param_type = child
                        .child_by_field_name("type")
                        .map(|n| node_text(n, bytes).to_string())
                        .unwrap_or_default();
                    if !param_name.is_empty() && param_name != "self" && param_name != "cls" {
                        params.push(Param { name: param_name, type_: param_type });
                    }
                }
            }
            _ => {}
        }
    }
    params
}

impl LanguageParser for PythonParser {
    fn parse(&self, source: &str) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("failed to set Python language");

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

        // Coverage: documentable = class + public functions/methods (not starting with _)
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
from os.path import join
from myapp.models import User
import collections

class MyService:
    """Service class docstring."""

    def process(self, data: str) -> None:
        pass

    def _private(self):
        pass

def module_function(x: int) -> str:
    return str(x)
"#;

    #[test]
    fn test_python_imports() {
        let r = PythonParser::new().parse(SAMPLE);
        // from X import Y → record X (the module path)
        assert!(r.raw_imports.iter().any(|i| i.contains("myapp.models")),
            "imports: {:?}", r.raw_imports);
        assert!(r.raw_imports.iter().any(|i| i == "collections"),
            "imports: {:?}", r.raw_imports);
    }

    #[test]
    fn test_python_class_has_docstring() {
        let r = PythonParser::new().parse(SAMPLE);
        let cls = r.signatures.iter().find(|s| s.kind == "class")
            .expect("should find class");
        assert!(cls.has_docstring, "MyService has docstring");
    }

    #[test]
    fn test_python_public_method() {
        let r = PythonParser::new().parse(SAMPLE);
        assert!(r.signatures.iter().any(|s| s.name == "process"),
            "sigs: {:?}", r.signatures.iter().map(|s| &s.name).collect::<Vec<_>>());
    }

    #[test]
    fn test_python_private_method_excluded() {
        let r = PythonParser::new().parse(SAMPLE);
        assert!(!r.signatures.iter().any(|s| s.name == "_private"),
            "private methods should not be extracted");
    }

    #[test]
    fn test_python_module_function() {
        let r = PythonParser::new().parse(SAMPLE);
        assert!(r.signatures.iter().any(|s| s.name == "module_function"),
            "module-level functions should be extracted");
    }

    #[test]
    fn test_python_coverage() {
        let r = PythonParser::new().parse(SAMPLE);
        // MyService (has docstring) = 1 documented
        // process (no docstring), module_function (no docstring) = 0 documented
        // total documentable: class + public methods + module functions
        assert!(r.coverage > 0.0 && r.coverage <= 1.0,
            "coverage: {}", r.coverage);
        // Only 1 out of 3 documentable items has a docstring
        assert!(r.coverage < 0.5, "coverage should be low: {}", r.coverage);
    }
}
