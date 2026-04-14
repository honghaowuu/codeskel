use super::{LanguageParser, ParseResult};
use crate::models::Signature;

pub struct RubyParser;

impl RubyParser {
    pub fn new() -> Self {
        Self
    }
}

fn node_text<'a>(node: tree_sitter::Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Check if a preceding sibling comment node exists (Ruby `#` comments).
fn extract_doc_comment(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut comments: Vec<String> = Vec::new();
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "comment" {
            let text = node_text(s, bytes);
            if text.starts_with('#') {
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
            "call" => {
                self.handle_call(node);
                // Don't recurse further into call args
            }
            "class" => {
                self.handle_class(node);
            }
            "method" | "singleton_method" => {
                self.handle_method(node);
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_node(child);
                }
            }
        }
    }

    fn handle_call(&mut self, node: tree_sitter::Node) {
        // Check if this is a require_relative call
        // In tree-sitter-ruby, method is accessed via child_by_field_name("method")
        let method_node = match node.child_by_field_name("method") {
            Some(n) => n,
            None => return,
        };

        let method_name = node_text(method_node, self.bytes);
        if method_name != "require_relative" {
            return;
        }

        // Get the arguments
        let args = match node.child_by_field_name("arguments") {
            Some(a) => a,
            None => return,
        };

        // Find string node in arguments
        let mut cursor = args.walk();
        for child in args.children(&mut cursor) {
            if child.kind() == "string" {
                // Extract string content - look for string_content child
                let mut str_cursor = child.walk();
                for str_child in child.children(&mut str_cursor) {
                    if str_child.kind() == "string_content" {
                        let content = node_text(str_child, self.bytes);
                        if !content.is_empty() {
                            self.result.raw_imports.push(content.to_string());
                        }
                        return;
                    }
                }
                // Fallback: strip quotes from string node text
                let text = node_text(child, self.bytes);
                let stripped = text.trim_matches('\'').trim_matches('"');
                if !stripped.is_empty() {
                    self.result.raw_imports.push(stripped.to_string());
                }
                return;
            }
        }
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
            docstring_text: doc.clone(),
        });

        // Recurse into the class body for methods
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "method" || child.kind() == "singleton_method" {
                    self.handle_method(child);
                }
            }
        }
    }

    fn handle_method(&mut self, node: tree_sitter::Node) {
        let name = match node.child_by_field_name("name") {
            Some(n) => node_text(n, self.bytes).to_string(),
            None => return,
        };

        let doc = extract_doc_comment(node, self.bytes);
        let line = node.start_position().row + 1;

        self.result.signatures.push(Signature {
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
        });
    }
}

impl LanguageParser for RubyParser {
    fn parse(&self, source: &str) -> ParseResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .expect("failed to set Ruby language");

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
require_relative 'models/user'
require 'json'

# User service class
class UserService
  # Creates a new service
  def initialize(db)
    @db = db
  end

  def get_user(id)
    nil
  end
end
"#;

    #[test]
    fn test_ruby_require_relative() {
        let r = RubyParser::new().parse(SAMPLE);
        assert!(r.raw_imports.iter().any(|i| i == "models/user"),
            "imports: {:?}", r.raw_imports);
        assert!(!r.raw_imports.iter().any(|i| i == "json"),
            "bare require should be excluded: {:?}", r.raw_imports);
    }

    #[test]
    fn test_ruby_class_has_comment() {
        let r = RubyParser::new().parse(SAMPLE);
        let cls = r.signatures.iter().find(|s| s.kind == "class")
            .expect("should find class");
        assert!(cls.has_docstring, "UserService has # comment before it");
    }

    #[test]
    fn test_ruby_methods() {
        let r = RubyParser::new().parse(SAMPLE);
        assert!(r.signatures.iter().any(|s| s.name == "initialize"),
            "sigs: {:?}", r.signatures.iter().map(|s| &s.name).collect::<Vec<_>>());
        assert!(r.signatures.iter().any(|s| s.name == "get_user"));
    }
}
