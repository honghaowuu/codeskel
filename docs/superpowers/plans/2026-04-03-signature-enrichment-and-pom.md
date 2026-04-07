# Signature Enrichment + `codeskel pom` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `docstring_text` and structured `annotations` fields to `Signature`, and implement the `codeskel pom` subcommand for Maven POM extraction.

**Architecture:** Model-first (Option B): update `models.rs` + make all parsers compile, then enrich each parser, then add the independent `pom` command. Each parser change is independently testable. The `pom` command uses `roxmltree` and touches only new files plus wiring in `cli.rs` and `main.rs`.

**Tech Stack:** Rust, tree-sitter (existing), roxmltree (new), serde_json (existing), clap (existing)

**Spec:** `docs/superpowers/specs/2026-04-03-signature-enrichment-and-pom-design.md`

---

## File Map

| File | Change |
|---|---|
| `src/models.rs` | Add `Annotation` struct; update `Signature.annotations` type; add `Signature.docstring_text` field |
| `src/parsers/mod.rs` | Add shared `strip_block_comment` and `strip_line_comment` helpers |
| `src/parsers/java.rs` | Add `javadoc_text()`, `extract_annotations_from_node()`, `extract_annotation_default_value()`; populate both new fields |
| `src/parsers/typescript.rs` | Add `preceding_jsdoc_text()`, `extract_decorators()`; populate both new fields |
| `src/parsers/javascript.rs` | Same as TypeScript (copy pattern) |
| `src/parsers/csharp.rs` | Add `doc_comment_text()`, `extract_attributes()`; populate both new fields |
| `src/parsers/rust_lang.rs` | Add `doc_comment_text()`, `extract_attributes()`; populate both new fields |
| `src/parsers/cpp.rs` | Add `doc_comment_text()`, `extract_cpp_attributes()`; populate both new fields |
| `src/parsers/python.rs` | Add `docstring_body_text()`, `strip_python_docstring()`; populate `docstring_text` |
| `src/parsers/go.rs` | Add `preceding_comment_text()`; populate `docstring_text` |
| `src/parsers/ruby.rs` | Add `doc_comment_text()`; populate `docstring_text` |
| `Cargo.toml` | Add `roxmltree = "0.19"` |
| `src/cli.rs` | Add `PomArgs`; add `Commands::Pom(PomArgs)` variant |
| `src/commands/pom.rs` | New — full pom command implementation |
| `src/commands/mod.rs` | Add `pub mod pom;` |
| `src/main.rs` | Dispatch `Commands::Pom` |

---

## Task 1: Update `models.rs` — Add `Annotation` type and new `Signature` fields

**Files:**
- Modify: `src/models.rs`

This task changes the model and makes everything compile. All parsers currently use
`annotations: Vec::new()` (type-inferred), which will automatically become `Vec<Annotation>`.
However, adding `docstring_text` to `Signature` **breaks all struct literals** — every
`Signature { ... }` in every parser must gain `docstring_text: None`. Do all of that here.

- [ ] **Step 1: Add `Annotation` struct to `models.rs` (before `Signature`)**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub name: String,
    pub value: Option<String>,
}
```

- [ ] **Step 2: Update `Signature` in `models.rs`**

Change:
```rust
pub annotations: Vec<String>,
pub line: usize,
pub has_docstring: bool,
```
To:
```rust
pub annotations: Vec<Annotation>,
pub line: usize,
pub has_docstring: bool,
#[serde(skip_serializing_if = "Option::is_none")]
pub docstring_text: Option<String>,
```

- [ ] **Step 3: Fix the `signature_roundtrip` test in `models.rs`**

Add `docstring_text: None` to the `Signature` literal in the test.

- [ ] **Step 4: Add `docstring_text: None` to every `Signature { ... }` in all parsers**

Files to touch (add `docstring_text: None` after `has_docstring: ...` in every struct literal):
- `src/parsers/java.rs` — 6 occurrences (class, interface, enum, method, constructor, field)
- `src/parsers/typescript.rs` — find all Signature literals
- `src/parsers/javascript.rs` — find all Signature literals
- `src/parsers/csharp.rs` — find all Signature literals
- `src/parsers/rust_lang.rs` — find all Signature literals
- `src/parsers/cpp.rs` — find all Signature literals
- `src/parsers/python.rs` — find all Signature literals
- `src/parsers/go.rs` — find all Signature literals
- `src/parsers/ruby.rs` — find all Signature literals

- [ ] **Step 5: Verify it compiles**

```bash
cargo build 2>&1
```
Expected: clean build, no errors.

- [ ] **Step 6: Run existing tests to confirm no regressions**

```bash
cargo test 2>&1
```
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/models.rs src/parsers/
git commit -m "feat: add Annotation type and docstring_text field to Signature"
```

---

## Task 2: Add shared strip helpers to `parsers/mod.rs`

**Files:**
- Modify: `src/parsers/mod.rs`

- [ ] **Step 1: Write failing tests for strip helpers (add at bottom of `parsers/mod.rs`)**

```rust
#[cfg(test)]
mod strip_tests {
    use super::*;

    #[test]
    fn test_strip_block_comment_multiline() {
        let raw = "/**\n * Represents a user.\n * Does things.\n */";
        assert_eq!(strip_block_comment(raw), "Represents a user.\nDoes things.");
    }

    #[test]
    fn test_strip_block_comment_singleline() {
        assert_eq!(strip_block_comment("/** foo */"), "foo");
    }

    #[test]
    fn test_strip_block_comment_doxygen() {
        assert_eq!(strip_block_comment("/*! brief desc */"), "brief desc");
    }

    #[test]
    fn test_strip_line_comment_triple_slash() {
        assert_eq!(strip_line_comment("/// Returns the user.", "///"), "Returns the user.");
    }

    #[test]
    fn test_strip_line_comment_no_space() {
        assert_eq!(strip_line_comment("///Returns the user.", "///"), "Returns the user.");
    }

    #[test]
    fn test_strip_line_comment_hash() {
        assert_eq!(strip_line_comment("# A ruby comment.", "#"), "A ruby comment.");
    }
}
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test strip_tests 2>&1
```
Expected: compile errors (functions don't exist yet).

- [ ] **Step 3: Implement `strip_block_comment` and `strip_line_comment` in `parsers/mod.rs`**

Add after the existing `use` statements:

```rust
/// Strip `/** ... */` or `/*! ... */` block comment delimiters.
/// Removes leading ` * ` or ` *` from interior lines.
/// Trims leading/trailing blank lines from result.
pub fn strip_block_comment(raw: &str) -> String {
    let s = raw.trim();
    let s = s.strip_prefix("/**").or_else(|| s.strip_prefix("/*!")).unwrap_or(s);
    let s = if s.ends_with("*/") { &s[..s.len() - 2] } else { s };
    let lines: Vec<&str> = s
        .lines()
        .map(|line| {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("* ") {
                rest.trim_end()
            } else if let Some(rest) = t.strip_prefix('*') {
                rest.trim_end()
            } else {
                t
            }
        })
        .collect();
    let start = lines.iter().position(|l| !l.is_empty()).unwrap_or(0);
    let end = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map(|i| i + 1)
        .unwrap_or(lines.len());
    if start >= end {
        return String::new();
    }
    lines[start..end].join("\n")
}

/// Strip a line-comment prefix (e.g. `///`, `//`, `#`) from a single raw comment line.
/// Tries `"prefix "` first (with trailing space), then `"prefix"` alone.
pub fn strip_line_comment<'a>(raw: &'a str, prefix: &str) -> &'a str {
    let t = raw.trim();
    // strip_prefix requires a &str; build with_space to avoid allocating repeatedly
    let without = t.strip_prefix(prefix).unwrap_or(t);
    without.strip_prefix(' ').unwrap_or(without).trim_end()
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test strip_tests 2>&1
```
Expected: all 6 pass.

- [ ] **Step 5: Commit**

```bash
git add src/parsers/mod.rs
git commit -m "feat: add strip_block_comment and strip_line_comment helpers"
```

---

## Task 3: Java parser — annotation extraction + docstring text

**Files:**
- Modify: `src/parsers/java.rs`

- [ ] **Step 1: Write failing tests**

Add a new `ANNOTATED` constant and tests in `mod tests`:

```rust
const ANNOTATED: &str = r#"
@RestController
@RequestMapping("/api/v1/users")
public class UserController {
    /**
     * Get all users.
     * Returns paginated list.
     */
    @GetMapping
    public java.util.List<User> getAll() { return null; }

    @SuppressWarnings(value = "unchecked")
    public void suppressedMethod() {}
}
"#;

#[test]
fn test_java_annotation_value() {
    let r = JavaParser::new().parse(ANNOTATED);
    let cls = r.signatures.iter().find(|s| s.kind == "class").unwrap();
    assert_eq!(cls.annotations.len(), 2, "annotations: {:?}", cls.annotations);
    let rm = cls.annotations.iter().find(|a| a.name == "RequestMapping").unwrap();
    assert_eq!(rm.value.as_deref(), Some("/api/v1/users"));
    let rc = cls.annotations.iter().find(|a| a.name == "RestController").unwrap();
    assert!(rc.value.is_none());
}

#[test]
fn test_java_annotation_named_arg_is_null() {
    let r = JavaParser::new().parse(ANNOTATED);
    let m = r.signatures.iter().find(|s| s.name == "suppressedMethod").unwrap();
    let ann = m.annotations.iter().find(|a| a.name == "SuppressWarnings").unwrap();
    assert!(ann.value.is_none(), "named arg should be null");
}

#[test]
fn test_java_docstring_text() {
    let r = JavaParser::new().parse(ANNOTATED);
    let m = r.signatures.iter().find(|s| s.name == "getAll").unwrap();
    assert!(m.has_docstring);
    assert_eq!(
        m.docstring_text.as_deref(),
        Some("Get all users.\nReturns paginated list.")
    );
}

#[test]
fn test_java_no_docstring_text_is_none() {
    let r = JavaParser::new().parse(ANNOTATED);
    let m = r.signatures.iter().find(|s| s.name == "suppressedMethod").unwrap();
    assert!(!m.has_docstring);
    assert!(m.docstring_text.is_none());
}
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test java -- test_java_annotation test_java_docstring 2>&1
```
Expected: 4 failures (all assert on new fields that are empty/None).

- [ ] **Step 3: Add `javadoc_text` helper to `java.rs`**

Add after the existing `has_javadoc` function:

```rust
/// Extract and strip the Javadoc text from the preceding block_comment node.
fn javadoc_text(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "block_comment" {
            let text = node_text(s, bytes);
            if text.starts_with("/**") {
                return Some(crate::parsers::strip_block_comment(text));
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
```

- [ ] **Step 4: Add annotation extraction helpers to `java.rs`**

Add after `extract_modifiers`:

```rust
use crate::models::Annotation;

/// Extract the unnamed default argument value from an annotation_argument_list node.
/// Returns None if args are named, multiple, or absent.
fn extract_annotation_default_value(
    args_node: tree_sitter::Node,
    bytes: &[u8],
) -> Option<String> {
    let mut cursor = args_node.walk();
    let named: Vec<_> = args_node
        .children(&mut cursor)
        .filter(|c| c.is_named())
        .collect();
    if named.len() != 1 {
        return None;
    }
    match named[0].kind() {
        "string_literal" => {
            let text = node_text(named[0], bytes);
            Some(text.trim_matches('"').to_string())
        }
        // element_value_pair = named arg; any other single value is also extractable
        "element_value_pair" => None,
        _ => None,
    }
}

fn extract_annotations_from_node(
    modifiers_node: tree_sitter::Node,
    bytes: &[u8],
) -> Vec<Annotation> {
    let mut annotations = Vec::new();
    let mut cursor = modifiers_node.walk();
    for child in modifiers_node.children(&mut cursor) {
        match child.kind() {
            "annotation" => {
                let name = child
                    .child_by_field_name("name")
                    .map(|n| node_text(n, bytes).to_string())
                    .unwrap_or_default();
                let mut ac = child.walk();
                let value = child
                    .children(&mut ac)
                    .find(|c| c.kind() == "annotation_argument_list")
                    .and_then(|args| extract_annotation_default_value(args, bytes));
                annotations.push(Annotation { name, value });
            }
            "marker_annotation" => {
                let name = child
                    .child_by_field_name("name")
                    .map(|n| node_text(n, bytes).to_string())
                    .unwrap_or_default();
                annotations.push(Annotation { name, value: None });
            }
            _ => {}
        }
    }
    annotations
}

fn extract_annotations(node: tree_sitter::Node, bytes: &[u8]) -> Vec<Annotation> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            return extract_annotations_from_node(child, bytes);
        }
    }
    Vec::new()
}
```

- [ ] **Step 5: Wire up in all `Signature` constructions inside `Walker`**

For each handler (`handle_class`, `handle_interface`, `handle_enum`, `handle_method`, `handle_constructor`, `handle_field`), change:
```rust
annotations: Vec::new(),
...
has_docstring: has_doc,
docstring_text: None,
```
to:
```rust
annotations: extract_annotations(node, self.bytes),
...
has_docstring: has_doc,
docstring_text: if has_doc { javadoc_text(node, self.bytes) } else { None },
```

Note: `handle_field` creates Signatures inside a loop over `variable_declarator` children.
The `has_doc` and annotations are computed from the parent `field_declaration` node,
which is the `node` parameter to `handle_field`. Use the same `node` for both.

- [ ] **Step 6: Add `use crate::models::Annotation;` import at top of `java.rs`**

Change:
```rust
use crate::models::{Param, Signature};
```
To:
```rust
use crate::models::{Annotation, Param, Signature};
```

- [ ] **Step 7: Run tests**

```bash
cargo test java 2>&1
```
Expected: all Java tests pass including the 4 new ones.

- [ ] **Step 8: Commit**

```bash
git add src/parsers/java.rs
git commit -m "feat: extract Java annotations and docstring text"
```

---

## Task 4: TypeScript + JavaScript parsers — decorators + docstring text

**Files:**
- Modify: `src/parsers/typescript.rs`
- Modify: `src/parsers/javascript.rs`

TypeScript/JavaScript decorators are sibling `decorator` nodes **preceding** the declaration
(not inside `modifiers`). Walk backwards collecting them. JSDoc text comes from the
preceding `comment` node starting with `/**`.

- [ ] **Step 1: Write failing tests in `typescript.rs`**

Add at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const DECORATED: &str = r#"
/** Manages users. */
@Injectable()
@Controller('/users')
class UserService {
    /** Get one user by id. */
    @Get(':id')
    getUser(id: string): User { return null; }

    unannotated(): void {}
}
"#;

    #[test]
    fn test_ts_decorator_value() {
        let r = TypeScriptParser::new().parse(DECORATED);
        let cls = r.signatures.iter().find(|s| s.kind == "class").unwrap();
        let ctrl = cls.annotations.iter().find(|a| a.name == "Controller").unwrap();
        assert_eq!(ctrl.value.as_deref(), Some("/users"));
        let inj = cls.annotations.iter().find(|a| a.name == "Injectable").unwrap();
        assert!(inj.value.is_none());
    }

    #[test]
    fn test_ts_docstring_text() {
        let r = TypeScriptParser::new().parse(DECORATED);
        let cls = r.signatures.iter().find(|s| s.kind == "class").unwrap();
        assert_eq!(cls.docstring_text.as_deref(), Some("Manages users."));
        let m = r.signatures.iter().find(|s| s.name == "getUser").unwrap();
        assert_eq!(m.docstring_text.as_deref(), Some("Get one user by id."));
    }

    #[test]
    fn test_ts_no_docstring_is_none() {
        let r = TypeScriptParser::new().parse(DECORATED);
        let m = r.signatures.iter().find(|s| s.name == "unannotated").unwrap();
        assert!(m.docstring_text.is_none());
        assert!(m.annotations.is_empty());
    }
}
```

- [ ] **Step 2: Run to confirm failures**

```bash
cargo test typescript 2>&1
```
Expected: 3 failures.

- [ ] **Step 3: Add helpers to `typescript.rs`**

Add after `extract_import_source`:

```rust
use crate::models::Annotation;

/// Extract the JSDoc text from the preceding `/** */` comment node.
fn preceding_jsdoc_text(node: tree_sitter::Node, source: &str) -> Option<String> {
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        match p.kind() {
            "comment" => {
                return source.get(p.byte_range()).and_then(|s| {
                    if s.starts_with("/**") {
                        Some(crate::parsers::strip_block_comment(s))
                    } else {
                        None
                    }
                });
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

/// Extract the first string argument from a call_expression's arguments node.
fn extract_call_string_arg(args_node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut cursor = args_node.walk();
    let named: Vec<_> = args_node.children(&mut cursor).filter(|c| c.is_named()).collect();
    if named.len() == 1 {
        let child = named[0];
        if child.kind() == "string" {
            let text = node_text(child, bytes);
            return Some(text.trim_matches('"').trim_matches('\'').trim_matches('`').to_string());
        }
    }
    None
}

/// Walk backwards collecting decorator nodes preceding `node`.
fn extract_decorators(node: tree_sitter::Node, bytes: &[u8]) -> Vec<Annotation> {
    let mut annotations = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(s) = prev {
        if s.kind() == "decorator" {
            let mut cursor = s.walk();
            for child in s.children(&mut cursor) {
                match child.kind() {
                    "identifier" => {
                        annotations.push(Annotation {
                            name: node_text(child, bytes).to_string(),
                            value: None,
                        });
                    }
                    "call_expression" => {
                        let name = child
                            .child_by_field_name("function")
                            .map(|n| node_text(n, bytes).to_string())
                            .unwrap_or_default();
                        let value = child
                            .child_by_field_name("arguments")
                            .and_then(|args| extract_call_string_arg(args, bytes));
                        annotations.push(Annotation { name, value });
                    }
                    "member_expression" => {
                        annotations.push(Annotation {
                            name: node_text(child, bytes).to_string(),
                            value: None,
                        });
                    }
                    _ => {}
                }
            }
            prev = s.prev_sibling();
        } else if s.is_extra() {
            prev = s.prev_sibling();
        } else {
            break;
        }
    }
    annotations.reverse();
    annotations
}
```

- [ ] **Step 4: Wire up in the TypeScript `Walker` `handle_declaration` method**

In each `Signature { ... }` construction, change:
```rust
annotations: Vec::new(),
has_docstring: ...,
docstring_text: None,
```
to:
```rust
annotations: extract_decorators(decl_node, self.bytes),  // decl_node = the class/function node
has_docstring: ...,
docstring_text: if has_doc { preceding_jsdoc_text(decl_node, self.source) } else { None },
```

Note: `Walker` has both `self.bytes` and `self.source` fields.

- [ ] **Step 5: Add `use crate::models::Annotation;` import to `typescript.rs`**

Change:
```rust
use crate::models::Signature;
```
To:
```rust
use crate::models::{Annotation, Signature};
```

- [ ] **Step 6: Run TypeScript tests**

```bash
cargo test typescript 2>&1
```
Expected: all tests pass.

- [ ] **Step 7: Repeat steps 1-6 for `javascript.rs`** (identical pattern)

The JavaScript parser (`javascript.rs`) has the same AST structure as TypeScript for
decorators and JSDoc. Copy the same helpers and wiring.

Write similar tests first, then implement. The `Walker` in `javascript.rs` may use
slightly different node kinds — check by running tests and inspecting failures.

- [ ] **Step 8: Commit**

```bash
git add src/parsers/typescript.rs src/parsers/javascript.rs
git commit -m "feat: extract TS/JS decorators and docstring text"
```

---

## Task 5: C# parser — attributes + docstring text

**Files:**
- Modify: `src/parsers/csharp.rs`

C# attributes appear as `attribute_list` sibling nodes (e.g. `[HttpGet]`, `[Route("/api")]`).
C# doc comments are consecutive `///` line comment nodes, each a separate sibling.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
namespace Example {
    /// <summary>
    /// Manages users.
    /// </summary>
    [ApiController]
    [Route("/api/users")]
    public class UserController {
        /// <summary>Gets a user.</summary>
        [HttpGet("{id}")]
        public User GetUser(int id) { return null; }

        public void NoDoc() {}
    }
}
"#;

    #[test]
    fn test_csharp_attribute_value() {
        let r = CSharpParser::new().parse(SAMPLE);
        let cls = r.signatures.iter().find(|s| s.kind == "class").unwrap();
        let route = cls.annotations.iter().find(|a| a.name == "Route").unwrap();
        assert_eq!(route.value.as_deref(), Some("/api/users"));
        let api = cls.annotations.iter().find(|a| a.name == "ApiController").unwrap();
        assert!(api.value.is_none());
    }

    #[test]
    fn test_csharp_docstring_text() {
        let r = CSharpParser::new().parse(SAMPLE);
        let cls = r.signatures.iter().find(|s| s.kind == "class").unwrap();
        assert!(cls.docstring_text.as_deref().unwrap_or("").contains("Manages users."));
        let m = r.signatures.iter().find(|s| s.name == "GetUser").unwrap();
        assert_eq!(m.docstring_text.as_deref(), Some("<summary>Gets a user.</summary>"));
    }

    #[test]
    fn test_csharp_no_doc_is_none() {
        let r = CSharpParser::new().parse(SAMPLE);
        let m = r.signatures.iter().find(|s| s.name == "NoDoc").unwrap();
        assert!(m.docstring_text.is_none());
    }
}
```

- [ ] **Step 2: Run to confirm failures**

```bash
cargo test csharp 2>&1
```

- [ ] **Step 3: Add helpers to `csharp.rs`**

```rust
use crate::models::Annotation;

/// Collect all consecutive `///` comment lines preceding `node` and strip prefixes.
fn doc_comment_text(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(s) = prev {
        if s.kind() == "comment" {
            let text = node_text(s, bytes);
            if text.trim().starts_with("///") {
                lines.push(text.to_string());
                prev = s.prev_sibling();
                continue;
            }
        } else if s.is_extra() {
            prev = s.prev_sibling();
            continue;
        }
        break;
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    Some(
        lines
            .iter()
            .map(|l| crate::parsers::strip_line_comment(l, "///").to_string())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
    )
}

/// Extract C# `[Attribute]` or `[Attribute("value")]` sibling nodes.
fn extract_attributes(node: tree_sitter::Node, bytes: &[u8]) -> Vec<Annotation> {
    let mut annotations = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(s) = prev {
        if s.kind() == "attribute_list" {
            let mut cursor = s.walk();
            for child in s.children(&mut cursor) {
                if child.kind() == "attribute" {
                    let name = child
                        .child_by_field_name("name")
                        .map(|n| node_text(n, bytes).to_string())
                        .unwrap_or_default();
                    // Look for attribute_argument_clause → single string_literal
                    let mut ac = child.walk();
                    let value = child
                        .children(&mut ac)
                        .find(|c| c.kind() == "attribute_argument_clause")
                        .and_then(|clause| {
                            let mut cc = clause.walk();
                            let args: Vec<_> = clause
                                .children(&mut cc)
                                .filter(|c| c.kind() == "attribute_argument")
                                .collect();
                            if args.len() == 1 {
                                let mut ac2 = args[0].walk();
                                args[0]
                                    .children(&mut ac2)
                                    .find(|c| c.kind() == "string_literal")
                                    .map(|n| node_text(n, bytes).trim_matches('"').to_string())
                            } else {
                                None
                            }
                        });
                    if !name.is_empty() {
                        annotations.push(Annotation { name, value });
                    }
                }
            }
            prev = s.prev_sibling();
        } else if s.is_extra() {
            prev = s.prev_sibling();
        } else {
            break;
        }
    }
    annotations.reverse();
    annotations
}
```

- [ ] **Step 4: Wire up in all `Signature` constructions in `csharp.rs`**

For each handler, change `annotations: Vec::new()` and `docstring_text: None` to:
```rust
annotations: extract_attributes(node, self.bytes),
has_docstring: ...,
docstring_text: if has_doc { doc_comment_text(node, self.bytes) } else { None },
```

- [ ] **Step 5: Add import, run tests, commit**

```bash
cargo test csharp 2>&1
# all pass
git add src/parsers/csharp.rs
git commit -m "feat: extract C# attributes and docstring text"
```

---

## Task 6: Rust parser — attributes + docstring text

**Files:**
- Modify: `src/parsers/rust_lang.rs`

Rust attributes are `attribute_item` sibling nodes (`#[derive(Debug)]`).
Doc comments are consecutive `///` or `//!` line comment nodes.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
/// A user struct.
/// Holds user data.
#[derive(Debug)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: u64,
}

/// Get a user.
#[allow(unused)]
pub fn get_user(id: u64) -> User { User { id } }

pub fn no_doc() {}
"#;

    #[test]
    fn test_rust_attribute_name() {
        let r = RustParser::new().parse(SAMPLE);
        let s = r.signatures.iter().find(|s| s.name == "User").unwrap();
        assert!(s.annotations.iter().any(|a| a.name == "derive"));
        assert!(s.annotations.iter().any(|a| a.name == "serde"));
    }

    #[test]
    fn test_rust_docstring_text() {
        let r = RustParser::new().parse(SAMPLE);
        let s = r.signatures.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(s.docstring_text.as_deref(), Some("A user struct.\nHolds user data."));
        let f = r.signatures.iter().find(|s| s.name == "get_user").unwrap();
        assert_eq!(f.docstring_text.as_deref(), Some("Get a user."));
    }

    #[test]
    fn test_rust_no_doc_is_none() {
        let r = RustParser::new().parse(SAMPLE);
        let f = r.signatures.iter().find(|s| s.name == "no_doc").unwrap();
        assert!(f.docstring_text.is_none());
    }
}
```

- [ ] **Step 2: Run to confirm failures**

```bash
cargo test rust_lang 2>&1
```

- [ ] **Step 3: Add helpers to `rust_lang.rs`**

```rust
use crate::models::Annotation;

/// Collect all consecutive `///`/`//!` line comment nodes preceding `node`.
fn doc_comment_text(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(s) = prev {
        match s.kind() {
            "line_comment" => {
                let text = node_text(s, bytes);
                if text.trim_start().starts_with("///") || text.trim_start().starts_with("//!") {
                    lines.push(text.to_string());
                    prev = s.prev_sibling();
                    continue;
                }
                break;
            }
            "block_comment" => {
                let text = node_text(s, bytes);
                if text.starts_with("/**") {
                    lines.push(text.to_string());
                }
                break;
            }
            _ if s.is_extra() => {
                prev = s.prev_sibling();
                continue;
            }
            _ => break,
        }
    }
    if lines.is_empty() {
        return None;
    }
    if lines.len() == 1 && lines[0].starts_with("/**") {
        return Some(crate::parsers::strip_block_comment(&lines[0]));
    }
    lines.reverse();
    let prefix = if lines[0].trim_start().starts_with("//!") { "//!" } else { "///" };
    Some(
        lines
            .iter()
            .map(|l| crate::parsers::strip_line_comment(l, prefix).to_string())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
    )
}

/// Extract `#[attr]` attribute_item nodes preceding `node`.
fn extract_attributes(node: tree_sitter::Node, bytes: &[u8]) -> Vec<Annotation> {
    let mut annotations = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(s) = prev {
        if s.kind() == "attribute_item" {
            let mut cursor = s.walk();
            for child in s.children(&mut cursor) {
                if child.kind() == "attribute" {
                    // name = first identifier/type_identifier child
                    let name = find_name_child(child, bytes)
                        .unwrap_or_default()
                        .to_string();
                    // value = token_tree with single string literal
                    let mut ac = child.walk();
                    let value = child
                        .children(&mut ac)
                        .find(|c| c.kind() == "token_tree")
                        .and_then(|tt| {
                            let mut tc = tt.walk();
                            let named: Vec<_> = tt
                                .children(&mut tc)
                                .filter(|c| c.kind() == "string_literal")
                                .collect();
                            if named.len() == 1 {
                                Some(node_text(named[0], bytes).trim_matches('"').to_string())
                            } else {
                                None
                            }
                        });
                    if !name.is_empty() {
                        annotations.push(Annotation { name, value });
                    }
                }
            }
            prev = s.prev_sibling();
        } else if s.kind() == "line_comment" || s.kind() == "block_comment" || s.is_extra() {
            // doc comments can appear between attribute_items and the declaration
            prev = s.prev_sibling();
        } else {
            break;
        }
    }
    annotations.reverse();
    annotations
}
```

- [ ] **Step 4: Wire up in all `Signature` constructions in `rust_lang.rs`**

For each handler, change to:
```rust
annotations: extract_attributes(node, self.bytes),
has_docstring: ...,
docstring_text: if has_doc { doc_comment_text(node, self.bytes) } else { None },
```

- [ ] **Step 5: Add import, run tests, commit**

```bash
cargo test rust_lang 2>&1
# all pass
git add src/parsers/rust_lang.rs
git commit -m "feat: extract Rust attributes and docstring text"
```

---

## Task 7: C++ parser — `[[attr]]` extraction + docstring text

**Files:**
- Modify: `src/parsers/cpp.rs`

C++ attributes use `[[nodiscard]]` / `[[deprecated("msg")]]` syntax. In tree-sitter-cpp,
these may appear as `attribute_declaration` sibling nodes or inline `attributed_*` nodes.
**Tip:** If tests fail due to wrong node kind names, add `eprintln!("{}", node.to_sexp());`
in the Walker to inspect the actual AST. The `has_doc_comment` function already works —
`doc_comment_text` follows the same pattern.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
/**
 * A point struct.
 */
struct Point {
    int x;
    int y;
};

/** Get the x value. */
[[nodiscard]]
int getX(const Point& p);

[[deprecated("use getX instead")]]
int get_x_old(const Point& p);

void no_doc();
"#;

    #[test]
    fn test_cpp_docstring_text() {
        let r = CppParser::new().parse(SAMPLE);
        let f = r.signatures.iter().find(|s| s.name == "getX").unwrap();
        assert_eq!(f.docstring_text.as_deref(), Some("Get the x value."));
    }

    #[test]
    fn test_cpp_no_doc_is_none() {
        let r = CppParser::new().parse(SAMPLE);
        let f = r.signatures.iter().find(|s| s.name == "no_doc").unwrap();
        assert!(f.docstring_text.is_none());
    }

    #[test]
    fn test_cpp_attribute_nodiscard() {
        let r = CppParser::new().parse(SAMPLE);
        let f = r.signatures.iter().find(|s| s.name == "getX").unwrap();
        assert!(f.annotations.iter().any(|a| a.name == "nodiscard"), "annotations: {:?}", f.annotations);
    }

    #[test]
    fn test_cpp_attribute_deprecated_value() {
        let r = CppParser::new().parse(SAMPLE);
        let f = r.signatures.iter().find(|s| s.name == "get_x_old").unwrap();
        let dep = f.annotations.iter().find(|a| a.name == "deprecated");
        assert!(dep.is_some());
        assert_eq!(dep.unwrap().value.as_deref(), Some("use getX instead"));
    }
}
```

- [ ] **Step 2: Run to confirm failures**

```bash
cargo test cpp 2>&1
```

- [ ] **Step 3: Add `doc_comment_text` to `cpp.rs`**

```rust
use crate::models::Annotation;

fn doc_comment_text(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "comment" {
            let text = node_text(s, bytes);
            if text.starts_with("/**") || text.starts_with("/*!") {
                return Some(crate::parsers::strip_block_comment(text));
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
```

- [ ] **Step 4: Add `extract_cpp_attributes` to `cpp.rs`**

In tree-sitter-cpp, `[[nodiscard]]` is an `attribute_declaration` node. The internal
structure is roughly:
```
(attribute_declaration "[[" (attribute_list (attribute (identifier))) "]]")
```
Walk backward collecting `attribute_declaration` siblings:

```rust
fn extract_cpp_attributes(node: tree_sitter::Node, bytes: &[u8]) -> Vec<Annotation> {
    let mut annotations = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(s) = prev {
        if s.kind() == "attribute_declaration" {
            // Walk into attribute_list → attribute
            let mut cursor = s.walk();
            for child in s.children(&mut cursor) {
                if child.kind() == "attribute_list" || child.kind() == "attribute" {
                    collect_cpp_attr(child, bytes, &mut annotations);
                }
            }
            prev = s.prev_sibling();
        } else if s.is_extra() || s.kind() == "comment" {
            prev = s.prev_sibling();
        } else {
            break;
        }
    }
    annotations.reverse();
    annotations
}

fn collect_cpp_attr(node: tree_sitter::Node, bytes: &[u8], out: &mut Vec<Annotation>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute" {
            let name = child
                .child_by_field_name("name")
                .or_else(|| {
                    // fallback: first identifier child
                    let mut c = child.walk();
                    child.children(&mut c).find(|n| n.kind() == "identifier")
                })
                .map(|n| node_text(n, bytes).to_string())
                .unwrap_or_default();
            // Look for attribute_arguments with a single string_literal
            let mut ac = child.walk();
            let value = child
                .children(&mut ac)
                .find(|c| c.kind() == "attribute_arguments" || c.kind() == "argument_list")
                .and_then(|args| {
                    let mut vc = args.walk();
                    let strings: Vec<_> = args
                        .children(&mut vc)
                        .filter(|c| c.kind() == "string_literal")
                        .collect();
                    if strings.len() == 1 {
                        Some(node_text(strings[0], bytes).trim_matches('"').to_string())
                    } else {
                        None
                    }
                });
            if !name.is_empty() {
                out.push(Annotation { name, value });
            }
        } else if child.kind() == "attribute_list" {
            collect_cpp_attr(child, bytes, out);
        }
    }
}
```

**Important:** C++ attribute node kinds vary by tree-sitter-cpp version. If tests fail,
add `eprintln!("{}", node.to_sexp());` to `handle_function` and re-run with
`cargo test cpp -- --nocapture` to see the actual AST shape. Adjust node kind strings
in `collect_cpp_attr` accordingly.

- [ ] **Step 5: Wire up in all `Signature` constructions in `cpp.rs`**

```rust
annotations: extract_cpp_attributes(node, self.bytes),
has_docstring: ...,
docstring_text: if has_doc { doc_comment_text(node, self.bytes) } else { None },
```

- [ ] **Step 6: Add import, run tests, commit**

```bash
cargo test cpp 2>&1
# all pass (iterate on node kinds if needed)
git add src/parsers/cpp.rs
git commit -m "feat: extract C++ attributes and docstring text"
```

---

## Task 8: Python parser — docstring text

**Files:**
- Modify: `src/parsers/python.rs`

Python has no annotations in the Java sense. Only `docstring_text` needs adding.
The body docstring is already detected by `first_body_is_docstring` — extract its text.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
class UserService:
    """Manages users.
    Provides CRUD operations."""

    def get_user(self, user_id: int):
        """Get a user by ID."""
        pass

    def no_doc(self):
        pass
"#;

    #[test]
    fn test_python_class_docstring_text() {
        let r = PythonParser::new().parse(SAMPLE);
        let cls = r.signatures.iter().find(|s| s.kind == "class").unwrap();
        assert!(cls.has_docstring);
        let text = cls.docstring_text.as_deref().unwrap_or("");
        assert!(text.contains("Manages users."), "got: {:?}", text);
    }

    #[test]
    fn test_python_method_docstring_text() {
        let r = PythonParser::new().parse(SAMPLE);
        let m = r.signatures.iter().find(|s| s.name == "get_user").unwrap();
        assert_eq!(m.docstring_text.as_deref(), Some("Get a user by ID."));
    }

    #[test]
    fn test_python_no_doc_is_none() {
        let r = PythonParser::new().parse(SAMPLE);
        let m = r.signatures.iter().find(|s| s.name == "no_doc").unwrap();
        assert!(m.docstring_text.is_none());
    }
}
```

- [ ] **Step 2: Run to confirm failures**

```bash
cargo test python 2>&1
```

- [ ] **Step 3: Add helpers to `python.rs`**

```rust
/// Extract and strip Python docstring text from a function/class body node.
/// Returns None if no docstring is present.
fn docstring_text_from_body(body: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "expression_statement" => {
                if let Some(inner) = child.child(0) {
                    if inner.kind() == "string" {
                        let raw = node_text(inner, bytes);
                        return Some(strip_python_docstring(raw));
                    }
                }
                return None;
            }
            "comment" | "\n" => continue,
            _ => return None,
        }
    }
    None
}

fn strip_python_docstring(raw: &str) -> String {
    let s = raw.trim();
    let s = if s.starts_with("\"\"\"") {
        s.strip_prefix("\"\"\"").unwrap_or(s)
    } else if s.starts_with("'''") {
        s.strip_prefix("'''").unwrap_or(s)
    } else if s.starts_with('"') {
        s.strip_prefix('"').unwrap_or(s)
    } else if s.starts_with('\'') {
        s.strip_prefix('\'').unwrap_or(s)
    } else {
        s
    };
    let s = if s.ends_with("\"\"\"") {
        &s[..s.len() - 3]
    } else if s.ends_with("'''") {
        &s[..s.len() - 3]
    } else if s.ends_with('"') {
        &s[..s.len() - 1]
    } else if s.ends_with('\'') {
        &s[..s.len() - 1]
    } else {
        s
    };
    s.trim().to_string()
}
```

- [ ] **Step 4: Wire up in `handle_class` and `handle_function`**

The Python parser has a `body` node. After computing `has_doc`:
```rust
let docstring_text = if has_doc {
    body_node.and_then(|b| docstring_text_from_body(b, self.bytes))
} else {
    None
};
```
Where `body_node` is the body of the class or function. Then set in the Signature.

- [ ] **Step 5: Run tests, commit**

```bash
cargo test python 2>&1
git add src/parsers/python.rs
git commit -m "feat: extract Python docstring text"
```

---

## Task 9: Go + Ruby parsers — comment text

**Files:**
- Modify: `src/parsers/go.rs`
- Modify: `src/parsers/ruby.rs`

No annotations for either language. Only `docstring_text`.

- [ ] **Step 1: Write failing tests for Go**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
package example

// User represents a user.
// It holds user data.
type User struct {
    ID int
}

// GetUser returns a user by ID.
func GetUser(id int) *User { return nil }

func noDoc() {}
"#;

    #[test]
    fn test_go_struct_docstring_text() {
        let r = GoParser::new().parse(SAMPLE);
        let s = r.signatures.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(s.docstring_text.as_deref(), Some("User represents a user.\nIt holds user data."));
    }

    #[test]
    fn test_go_func_docstring_text() {
        let r = GoParser::new().parse(SAMPLE);
        let f = r.signatures.iter().find(|s| s.name == "GetUser").unwrap();
        assert_eq!(f.docstring_text.as_deref(), Some("GetUser returns a user by ID."));
    }

    #[test]
    fn test_go_no_doc_is_none() {
        let r = GoParser::new().parse(SAMPLE);
        let f = r.signatures.iter().find(|s| s.name == "noDoc").unwrap();
        assert!(f.docstring_text.is_none());
    }
}
```

- [ ] **Step 2: Run to confirm failures**

```bash
cargo test go 2>&1
```

- [ ] **Step 3: Add `preceding_comment_text` to `go.rs`**

```rust
fn preceding_comment_text(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "comment" {
            lines.push(node_text(p, bytes).to_string());
            prev = p.prev_sibling();
        } else if p.is_extra() {
            prev = p.prev_sibling();
        } else {
            break;
        }
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    Some(
        lines
            .iter()
            .map(|l| crate::parsers::strip_line_comment(l, "//").to_string())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
    )
}
```

- [ ] **Step 4: Wire up in Go `Walker` handlers**

For each `Signature { ... }`, change:
```rust
has_docstring: has_doc,
docstring_text: if has_doc { preceding_comment_text(node, self.bytes) } else { None },
```

- [ ] **Step 5: Run Go tests**

```bash
cargo test go 2>&1
```

- [ ] **Step 6: Write failing tests for Ruby** (add at bottom of `ruby.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# Manages users.
# Provides CRUD.
class UserService
  # Get a user by ID.
  def get_user(id)
  end

  def no_doc
  end
end
"#;

    #[test]
    fn test_ruby_class_docstring_text() {
        let r = RubyParser::new().parse(SAMPLE);
        let cls = r.signatures.iter().find(|s| s.kind == "class").unwrap();
        assert_eq!(cls.docstring_text.as_deref(), Some("Manages users.\nProvides CRUD."));
    }

    #[test]
    fn test_ruby_method_docstring_text() {
        let r = RubyParser::new().parse(SAMPLE);
        let m = r.signatures.iter().find(|s| s.name == "get_user").unwrap();
        assert_eq!(m.docstring_text.as_deref(), Some("Get a user by ID."));
    }

    #[test]
    fn test_ruby_no_doc_is_none() {
        let r = RubyParser::new().parse(SAMPLE);
        let m = r.signatures.iter().find(|s| s.name == "no_doc").unwrap();
        assert!(m.docstring_text.is_none());
    }
}
```

- [ ] **Step 7: Run to confirm failures**

```bash
cargo test ruby 2>&1
```

- [ ] **Step 8: Add `doc_comment_text` to `ruby.rs`**

```rust
fn doc_comment_text(node: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut prev = node.prev_sibling();
    while let Some(s) = prev {
        if s.kind() == "comment" {
            let text = node_text(s, bytes);
            if text.trim_start().starts_with('#') {
                lines.push(text.to_string());
                prev = s.prev_sibling();
                continue;
            }
        } else if s.is_extra() {
            prev = s.prev_sibling();
            continue;
        }
        break;
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    Some(
        lines
            .iter()
            .map(|l| crate::parsers::strip_line_comment(l, "#").to_string())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
    )
}
```

- [ ] **Step 9: Wire up in Ruby `Walker` handlers**

For each `Signature { ... }`:
```rust
has_docstring: has_doc,
docstring_text: if has_doc { doc_comment_text(node, self.bytes) } else { None },
```

- [ ] **Step 10: Run Ruby tests, then run all tests**

```bash
cargo test ruby 2>&1
cargo test 2>&1
```
Expected: all tests pass.

- [ ] **Step 11: Commit**

```bash
git add src/parsers/go.rs src/parsers/ruby.rs
git commit -m "feat: extract Go and Ruby docstring text"
```

---

## Task 10: Add `codeskel pom` command

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/cli.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/main.rs`
- Create: `src/commands/pom.rs`

The `pom` command is fully independent of the signature changes above.

- [ ] **Step 1: Add `roxmltree` to `Cargo.toml`**

In the `[dependencies]` section, add:
```toml
roxmltree = "0.19"
```

- [ ] **Step 2: Add `PomArgs` and `Commands::Pom` to `cli.rs`**

Add to `Commands` enum:
```rust
/// Extract Maven project metadata from pom.xml
Pom(PomArgs),
```

Add new args struct:
```rust
#[derive(Args, Debug)]
pub struct PomArgs {
    /// Path to the project root (default: current directory)
    #[arg(default_value = ".")]
    pub project_root: std::path::PathBuf,

    /// Path hint for multi-module resolution
    #[arg(long)]
    pub controller_path: Option<String>,
}
```

- [ ] **Step 3: Add `pub mod pom;` to `commands/mod.rs`**

- [ ] **Step 4: Add dispatch in `main.rs`**

```rust
Commands::Pom(args) => codeskel::commands::pom::run(args),
```

- [ ] **Step 5: Create `src/commands/pom.rs` — data types + unit-testable helpers**

```rust
use crate::cli::PomArgs;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct PomOutput {
    pub service_name: String,
    pub group_id: String,
    pub version: String,
    pub pom_path: String,
    pub is_multi_module: bool,
    pub internal_sdk_deps: Vec<SdkDep>,
    pub existing_skill_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SdkDep {
    pub artifact_id: String,
    pub group_id: String,
    pub version: String,
}

pub fn run(args: PomArgs) -> anyhow::Result<bool> {
    let project_root = args.project_root;
    let root_pom_path = project_root.join("pom.xml");
    if !root_pom_path.exists() {
        anyhow::bail!("No pom.xml found in {}", project_root.display());
    }
    let root_content = std::fs::read_to_string(&root_pom_path)
        .map_err(|e| anyhow::anyhow!("Cannot read {}: {}", root_pom_path.display(), e))?;

    let is_multi_module = {
        let doc = roxmltree::Document::parse(&root_content)
            .map_err(|e| anyhow::anyhow!("XML parse error in {}: {}", root_pom_path.display(), e))?;
        xml_child(&doc.root_element(), "modules").is_some()
    };

    let output = if is_multi_module {
        let controller_path = args.controller_path.ok_or_else(|| {
            anyhow::anyhow!(
                "Multi-module POM detected — --controller-path is required to select a sub-module"
            )
        })?;
        let sub_dir = find_submodule_dir(&root_content, &project_root, &controller_path)?;
        let module_pom_path = sub_dir.join("pom.xml");
        let module_content = std::fs::read_to_string(&module_pom_path)
            .map_err(|e| anyhow::anyhow!("Cannot read {}: {}", module_pom_path.display(), e))?;
        extract_output(&module_pom_path, &module_content, Some(&root_content), true)?
    } else {
        extract_output(&root_pom_path, &root_content, None, false)?
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(false)
}

/// Find the sub-module directory whose path contains `controller_path`.
fn find_submodule_dir(
    root_content: &str,
    project_root: &Path,
    controller_path: &str,
) -> anyhow::Result<PathBuf> {
    let doc = roxmltree::Document::parse(root_content)?;
    let root_el = doc.root_element();
    let modules = xml_child(&root_el, "modules")
        .ok_or_else(|| anyhow::anyhow!("No <modules> element in root POM"))?;
    let mut cursor = modules.walk();
    for node in modules.traverse() {
        let n = match node { roxmltree::NodeEdge::Start(n) => n, _ => continue };
        if n.tag_name().name() == "module" {
            if let Some(text) = n.text() {
                let sub_dir = project_root.join(text);
                // Check if controller_path is under this sub-module directory
                let abs_ctrl = project_root.join(controller_path);
                if abs_ctrl.starts_with(&sub_dir) || controller_path.contains(text) {
                    return Ok(sub_dir);
                }
            }
        }
    }
    anyhow::bail!(
        "No sub-module found containing '{}'. Modules in root POM: check <modules> block.",
        controller_path
    )
}

fn extract_output(
    pom_path: &Path,
    content: &str,
    parent_content: Option<&str>,
    is_multi_module: bool,
) -> anyhow::Result<PomOutput> {
    let doc = roxmltree::Document::parse(content)
        .map_err(|e| anyhow::anyhow!("XML parse error in {}: {}", pom_path.display(), e))?;
    let root = doc.root_element();

    let parent_doc_storage;
    let parent_root: Option<roxmltree::Node> = if let Some(pc) = parent_content {
        parent_doc_storage = roxmltree::Document::parse(pc)
            .map_err(|e| anyhow::anyhow!("XML parse error in parent POM: {}", e))?;
        Some(parent_doc_storage.root_element())
    } else {
        None
    };
    // Note: parent_doc_storage must live as long as parent_root.
    // Rust's borrow checker enforces this — do not move parent_doc_storage after this point.

    let artifact_id = xml_text(&root, "artifactId").ok_or_else(|| {
        anyhow::anyhow!("Missing <artifactId> in POM at {}", pom_path.display())
    })?;

    let group_id = xml_text(&root, "groupId")
        .or_else(|| parent_root.as_ref().and_then(|p| xml_text(p, "groupId")))
        .ok_or_else(|| anyhow::anyhow!("Missing <groupId> in POM and parent POM"))?;

    let raw_version = xml_text(&root, "version")
        .or_else(|| parent_root.as_ref().and_then(|p| xml_text(p, "version")))
        .unwrap_or_default();
    let version = resolve_property(&raw_version, &root, parent_root.as_ref());

    // Root groupId for SDK dep filtering: use parent groupId if multi-module, else own groupId
    let root_group_id = parent_root
        .as_ref()
        .and_then(|p| xml_text(p, "groupId"))
        .unwrap_or_else(|| group_id.clone());

    let internal_sdk_deps = extract_sdk_deps(&root, parent_root.as_ref(), &root_group_id);

    let pom_dir = pom_path.parent().unwrap_or(pom_path);
    let skill_path = pom_dir.join("docs").join("skills").join(&artifact_id).join("SKILL.md");
    let existing_skill_path = if skill_path.exists() {
        Some(
            skill_path
                .canonicalize()
                .unwrap_or(skill_path)
                .display()
                .to_string(),
        )
    } else {
        None
    };

    let abs_pom = pom_path
        .canonicalize()
        .unwrap_or_else(|_| pom_path.to_path_buf());

    Ok(PomOutput {
        service_name: artifact_id,
        group_id,
        version,
        pom_path: abs_pom.display().to_string(),
        is_multi_module,
        internal_sdk_deps,
        existing_skill_path,
    })
}

fn extract_sdk_deps(
    module: &roxmltree::Node,
    parent: Option<&roxmltree::Node>,
    root_group_id: &str,
) -> Vec<SdkDep> {
    let mut deps = Vec::new();
    for pom_node in [Some(module), parent].iter().flatten() {
        if let Some(dependencies) = xml_child(pom_node, "dependencies") {
            for dep in dependencies
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "dependency")
            {
                let artifact_id = match xml_text(&dep, "artifactId") {
                    Some(id) => id,
                    None => continue,
                };
                let dep_group = match xml_text(&dep, "groupId") {
                    Some(g) => g,
                    None => continue,
                };
                let raw_ver = xml_text(&dep, "version").unwrap_or_default();
                let version = resolve_property(&raw_ver, module, parent);

                let is_sdk =
                    artifact_id.ends_with("-api") || artifact_id.ends_with("-sdk");
                let is_internal = dep_group == root_group_id
                    || dep_group.starts_with(&format!("{}.", root_group_id));

                if is_sdk && is_internal {
                    deps.push(SdkDep {
                        artifact_id,
                        group_id: dep_group,
                        version,
                    });
                }
            }
        }
    }
    deps
}

/// Resolve a `${prop}` reference from module then parent `<properties>`.
fn resolve_property(value: &str, module: &roxmltree::Node, parent: Option<&roxmltree::Node>) -> String {
    if !value.starts_with("${") || !value.ends_with('}') {
        return value.to_string();
    }
    let prop_name = &value[2..value.len() - 1];
    for node in [Some(module), parent].iter().flatten() {
        if let Some(props) = xml_child(node, "properties") {
            if let Some(val) = xml_text(&props, prop_name) {
                return val;
            }
        }
    }
    value.to_string() // return raw ${...} if unresolvable
}

/// Get direct child element text.
fn xml_text(node: &roxmltree::Node, tag: &str) -> Option<String> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == tag)
        .and_then(|n| n.text())
        .map(|s| s.to_string())
}

/// Get a direct child element node.
fn xml_child<'a, 'b>(
    node: &roxmltree::Node<'a, 'b>,
    tag: &str,
) -> Option<roxmltree::Node<'a, 'b>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == tag)
}
```

**Note on lifetimes:** `roxmltree::Document` borrows from its input `&str`. `parent_doc_storage` is bound to the outer scope so the `parent_root` borrow is valid. Do not restructure these into separate functions that try to return `Document` or `Node` — keep them in the same scope.

- [ ] **Step 6: Write unit tests in `pom.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_POM: &str = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>billing-service</artifactId>
  <version>1.2.0</version>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>user-api</artifactId>
      <version>2.1.0</version>
    </dependency>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>core-lib</artifactId>
      <version>1.0.0</version>
    </dependency>
    <dependency>
      <groupId>org.springframework</groupId>
      <artifactId>spring-web-api</artifactId>
      <version>5.0</version>
    </dependency>
  </dependencies>
</project>"#;

    const PROP_POM: &str = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>billing-service</artifactId>
  <version>${billing.version}</version>
  <properties>
    <billing.version>3.0.0</billing.version>
  </properties>
</project>"#;

    #[test]
    fn test_simple_pom_extraction() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = tmp.path().join("pom.xml");
        std::fs::write(&pom_path, SIMPLE_POM).unwrap();
        let output = extract_output(&pom_path, SIMPLE_POM, None, false).unwrap();
        assert_eq!(output.service_name, "billing-service");
        assert_eq!(output.group_id, "com.example");
        assert_eq!(output.version, "1.2.0");
        assert!(!output.is_multi_module);
    }

    #[test]
    fn test_sdk_dep_filtering() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = tmp.path().join("pom.xml");
        std::fs::write(&pom_path, SIMPLE_POM).unwrap();
        let output = extract_output(&pom_path, SIMPLE_POM, None, false).unwrap();
        assert_eq!(output.internal_sdk_deps.len(), 1);
        assert_eq!(output.internal_sdk_deps[0].artifact_id, "user-api");
        // core-lib: not -api/-sdk suffix → excluded
        // spring-web-api: not com.example groupId → excluded
    }

    #[test]
    fn test_property_version_resolution() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = tmp.path().join("pom.xml");
        std::fs::write(&pom_path, PROP_POM).unwrap();
        let output = extract_output(&pom_path, PROP_POM, None, false).unwrap();
        assert_eq!(output.version, "3.0.0");
    }

    #[test]
    fn test_existing_skill_path_null() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = tmp.path().join("pom.xml");
        std::fs::write(&pom_path, SIMPLE_POM).unwrap();
        let output = extract_output(&pom_path, SIMPLE_POM, None, false).unwrap();
        assert!(output.existing_skill_path.is_none());
    }

    #[test]
    fn test_existing_skill_path_found() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = tmp.path().join("pom.xml");
        std::fs::write(&pom_path, SIMPLE_POM).unwrap();
        let skill_dir = tmp.path().join("docs/skills/billing-service");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# skill").unwrap();
        let output = extract_output(&pom_path, SIMPLE_POM, None, false).unwrap();
        assert!(output.existing_skill_path.is_some());
        assert!(output.existing_skill_path.unwrap().ends_with("SKILL.md"));
    }
}
```

- [ ] **Step 7: Run pom tests**

```bash
cargo test pom 2>&1
```
Expected: all pass.

- [ ] **Step 8: Smoke-test the CLI end-to-end**

Create a minimal `pom.xml` in a temp directory and run the binary:

```bash
mkdir -p /tmp/test-pom
cat > /tmp/test-pom/pom.xml << 'EOF'
<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>test-service</artifactId>
  <version>1.0.0</version>
</project>
EOF
cargo run -- pom /tmp/test-pom 2>&1
```

Expected: JSON output with `service_name: "test-service"`.

- [ ] **Step 9: Run full test suite**

```bash
cargo test 2>&1
```
Expected: all tests pass, no warnings.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml Cargo.lock src/cli.rs src/commands/mod.rs src/commands/pom.rs src/main.rs
git commit -m "feat: add codeskel pom subcommand for Maven POM extraction"
```

---

## Final Verification

- [ ] **Run all tests one more time**

```bash
cargo test 2>&1
```

- [ ] **Run clippy**

```bash
cargo clippy -- -D warnings 2>&1
```
Fix any warnings before proceeding.

- [ ] **Verify JSON output format matches PRD spec**

```bash
cargo run -- pom /tmp/test-pom 2>&1
# Check field names: service_name, group_id, version, pom_path, is_multi_module,
#                    internal_sdk_deps[].artifact_id, existing_skill_path
```

Also verify that `codeskel get` output on a scanned Java project now shows
`annotations: [{name: "...", value: ...}]` and `docstring_text: "..."` in the JSON.

- [ ] **Final commit if any fixes were needed**

```bash
git add -p
git commit -m "fix: address clippy warnings from signature enrichment and pom command"
```
