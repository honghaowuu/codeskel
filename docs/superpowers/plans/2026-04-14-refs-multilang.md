# Multi-Language `--refs` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `codeskel get --refs` from Java-only to Python, TypeScript/JavaScript, Go, and Rust using the same three-phase AST approach already established for Java.

**Architecture:** Each new language gets an independent `src/refs/<lang>.rs` file implementing `RefsAnalyzer`. A new `build_import_map` helper in `src/commands/get.rs` replaces the current file-stem logic with language-aware sig-name–based lookup for non-Java languages. Dispatch arms in `src/refs/mod.rs` wire each analyzer in.

**Tech Stack:** Rust, tree-sitter (tree-sitter-typescript, tree-sitter-python, tree-sitter-go, tree-sitter-rust already in Cargo.toml), existing `RefsAnalyzer` trait.

---

## File Map

| File | Change |
|---|---|
| `src/commands/get.rs` | Extract `build_import_map(lang, internal_imports, cache)` helper; call it in `compute_refs` |
| `src/refs/mod.rs` | Add `pub mod` for each new analyzer; add dispatch arms |
| `src/refs/ts.rs` | New — `TsRefsAnalyzer` (TypeScript + JavaScript) |
| `src/refs/python.rs` | New — `PythonRefsAnalyzer` |
| `src/refs/go.rs` | New — `GoRefsAnalyzer` |
| `src/refs/rust_lang.rs` | New — `RustRefsAnalyzer` |

---

## Task 1: Refactor `compute_refs` — extract `build_import_map`

**Files:**
- Modify: `src/commands/get.rs:158-167`

Currently `compute_refs` builds `import_map` using file stem for all languages. For Python/TS/JS/Go/Rust the import_map must be keyed by sig names (class/function/interface names from the dep's `signatures`), not file stems. Java keeps the stem-based behavior.

- [ ] **Step 1: Write failing test in `src/commands/get.rs` (in the existing `#[cfg(test)]` if any, else add one)**

No existing test module in `get.rs` — add it. The test verifies `build_import_map` returns sig-name keys for Python.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CacheFile, FileEntry, Signature, Stats};
    use std::collections::HashMap;

    fn make_cache(lang: &str, dep_sigs: Vec<(&str, &str)>) -> CacheFile {
        // dep file: "src/models.py" with given signatures
        let dep_sigs_models: Vec<Signature> = dep_sigs.iter().map(|(kind, name)| Signature {
            kind: kind.to_string(),
            name: name.to_string(),
            modifiers: vec![],
            params: None,
            return_type: None,
            throws: vec![],
            extends: None,
            implements: vec![],
            annotations: vec![],
            line: 1,
            has_docstring: false,
            docstring_text: None,
        }).collect();

        let dep_entry = FileEntry {
            path: "src/models.py".into(),
            language: "python".into(),
            package: None,
            comment_coverage: 0.0,
            skip: false,
            skip_reason: None,
            cycle_warning: false,
            internal_imports: vec![],
            signatures: dep_sigs_models,
            scanned_at: None,
        };

        let caller_entry = FileEntry {
            path: "src/service.py".into(),
            language: lang.into(),
            package: None,
            comment_coverage: 0.0,
            skip: false,
            skip_reason: None,
            cycle_warning: false,
            internal_imports: vec!["src/models.py".into()],
            signatures: vec![],
            scanned_at: None,
        };

        let mut files = HashMap::new();
        files.insert("src/models.py".into(), dep_entry);
        files.insert("src/service.py".into(), caller_entry);

        CacheFile {
            version: 1,
            scanned_at: "2026-01-01T00:00:00Z".into(),
            project_root: "/tmp".into(),
            detected_languages: vec![lang.into()],
            stats: Stats { total_files: 2, skipped_covered: 0, skipped_generated: 0, to_comment: 2 },
            min_docstring_words: 0,
            order: vec!["src/models.py".into(), "src/service.py".into()],
            files,
        }
    }

    #[test]
    fn test_build_import_map_python_uses_sig_names() {
        use crate::models::Language;
        let cache = make_cache("python", vec![("class", "User"), ("function", "get_user")]);
        let entry = cache.files.get("src/service.py").unwrap();
        let map = build_import_map(&Language::Python, &entry.internal_imports, &cache);
        assert!(map.contains_key("User"), "should contain class name; map: {:?}", map);
        assert!(map.contains_key("get_user"), "should contain function name; map: {:?}", map);
        assert!(!map.contains_key("models"), "should NOT use file stem for Python; map: {:?}", map);
    }

    #[test]
    fn test_build_import_map_java_uses_stem() {
        use crate::models::Language;
        let cache = make_cache("java", vec![("class", "User"), ("method", "findById")]);
        let entry = cache.files.get("src/service.py").unwrap(); // path doesn't matter, just needs internal_imports
        let map = build_import_map(&Language::Java, &entry.internal_imports, &cache);
        assert!(map.contains_key("models"), "Java should use file stem; map: {:?}", map);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```
cargo test -p codeskel test_build_import_map -- --nocapture
```
Expected: FAIL with "cannot find function `build_import_map`"

- [ ] **Step 3: Implement `build_import_map` and refactor `compute_refs`**

In `src/commands/get.rs`, add above `compute_refs`:

```rust
/// Builds import_map: simple_name → dep_file_path.
///
/// Java: uses file stem (filename == class name convention).
/// All other languages: uses sig names from the dep's FileEntry.signatures.
pub fn build_import_map(
    lang: &Language,
    internal_imports: &[String],
    cache: &crate::models::CacheFile,
) -> std::collections::HashMap<String, String> {
    let mut import_map = std::collections::HashMap::new();
    match lang {
        Language::Java => {
            for dep_path in internal_imports {
                if let Some(stem) = std::path::Path::new(dep_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                {
                    import_map.insert(stem.to_string(), dep_path.clone());
                }
            }
        }
        _ => {
            // Sig-name–based: map every exported name from dep signatures
            for dep_path in internal_imports {
                if let Some(dep_entry) = cache.files.get(dep_path) {
                    for sig in &dep_entry.signatures {
                        import_map.insert(sig.name.clone(), dep_path.clone());
                    }
                }
            }
        }
    }
    import_map
}
```

Then replace the entire body of `compute_refs` from line 153 onward with the reordered version below. Key change: `lang` is now derived **before** `import_map`, so it can be passed to `build_import_map`. Also note: the `if import_map.is_empty()` early-exit guard is **preserved** — it prevents a useless file read when there are no internal deps.

```rust
pub fn compute_refs(
    cache: &crate::models::CacheFile,
    file_path: &str,
) -> anyhow::Result<std::collections::HashMap<String, Vec<String>>> {
    let entry = cache.files.get(file_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found in cache", file_path))?;

    // Derive lang FIRST — needed by build_import_map
    let lang = Language::from_str(&entry.language)
        .map_err(|e| anyhow::anyhow!("Unknown language '{}': {}", entry.language, e))?;

    // Build import_map: simple_name → dep_file_path (language-specific strategy)
    let import_map = build_import_map(&lang, &entry.internal_imports, cache);

    // Build dep_sigs: dep_file_path → [all sig names regardless of kind]
    let mut dep_sigs = std::collections::HashMap::new();
    for dep_path in &entry.internal_imports {
        if let Some(dep_entry) = cache.files.get(dep_path) {
            let names: Vec<String> = dep_entry.signatures.iter()
                .map(|s| s.name.clone())
                .collect();
            dep_sigs.insert(dep_path.clone(), names);
        }
    }

    // No internal imports → empty refs (avoid reading source for nothing)
    if import_map.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    // Read source from disk
    let source_path = std::path::Path::new(&cache.project_root).join(file_path);
    let source = std::fs::read_to_string(&source_path)
        .map_err(|e| anyhow::anyhow!("Cannot read '{}': {}", source_path.display(), e))?;

    // Dispatch to language-specific analyzer
    match crate::refs::get_refs_analyzer(&lang) {
        Some(analyzer) => Ok(analyzer.extract_refs(&source, &import_map, &dep_sigs)),
        None => {
            eprintln!("[codeskel] --refs: language '{}' not yet supported; returning empty refs",
                entry.language);
            Ok(std::collections::HashMap::new())
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```
cargo test -p codeskel test_build_import_map -- --nocapture
cargo test -p codeskel 2>&1 | tail -5
```
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add src/commands/get.rs
git commit -m "refactor: extract build_import_map with language-specific logic"
```

---

## Task 2: TypeScript/JavaScript `RefsAnalyzer`

**Files:**
- Create: `src/refs/ts.rs`
- Modify: `src/refs/mod.rs`

### What to implement

**Phase 1 — binding map:** Walk `variable_declarator` nodes for typed `const x: Type = ...` and `new Type()` RHS; walk `required_parameter`/`optional_parameter` for typed params.

**Phase 2 — candidates:** `call_expression` with `member_expression` function → resolve object → emit `(resolved_type, property)`; `new_expression` → emit `(constructor, None)`; `member_expression` outside call → emit field access; `type_annotation → type_identifier` → emit `(type_name, None)`.

**Phase 3:** Same cross-reference as Java (already in `extract_refs` loop).

- [ ] **Step 1: Write failing tests in `src/refs/ts.rs`**

Create the file with tests first:

```rust
use std::collections::HashMap;
use super::RefsAnalyzer;

pub struct TsRefsAnalyzer;

impl TsRefsAnalyzer {
    pub fn new() -> Self { Self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ts_typed_param_ref() {
        let source = r#"
import { UserService } from './userService';

function handleUser(svc: UserService): void {
    svc.findUser("123");
}
"#;
        let mut import_map = HashMap::new();
        import_map.insert("UserService".to_string(), "src/userService.ts".to_string());
        let mut dep_sigs = HashMap::new();
        dep_sigs.insert("src/userService.ts".to_string(),
            vec!["UserService".to_string(), "findUser".to_string()]);

        let analyzer = TsRefsAnalyzer::new();
        let refs = analyzer.extract_refs(source, &import_map, &dep_sigs);

        let svc_refs = refs.get("src/userService.ts").expect("userService.ts must appear");
        assert!(svc_refs.contains(&"UserService".to_string()),
            "UserService type ref missing; got: {:?}", svc_refs);
        assert!(svc_refs.contains(&"findUser".to_string()),
            "findUser method ref missing; got: {:?}", svc_refs);
    }

    #[test]
    fn test_ts_new_expression_and_method() {
        let source = r#"
import { UserRepo } from './userRepo';

function main() {
    const repo = new UserRepo();
    repo.save({ name: 'Alice' });
}
"#;
        let mut import_map = HashMap::new();
        import_map.insert("UserRepo".to_string(), "src/userRepo.ts".to_string());
        let mut dep_sigs = HashMap::new();
        dep_sigs.insert("src/userRepo.ts".to_string(),
            vec!["UserRepo".to_string(), "save".to_string()]);

        let analyzer = TsRefsAnalyzer::new();
        let refs = analyzer.extract_refs(source, &import_map, &dep_sigs);

        let repo_refs = refs.get("src/userRepo.ts").expect("userRepo.ts must appear");
        assert!(repo_refs.contains(&"UserRepo".to_string()),
            "UserRepo constructor ref missing; got: {:?}", repo_refs);
        assert!(repo_refs.contains(&"save".to_string()),
            "save method missing; got: {:?}", repo_refs);
    }

    #[test]
    fn test_ts_no_internal_imports() {
        let source = "const x = 1;";
        let refs = TsRefsAnalyzer::new().extract_refs(
            source, &HashMap::new(), &HashMap::new());
        assert!(refs.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test -p codeskel --lib 2>&1 | grep -A2 "test_ts_"
```
Expected: compile error — `TsRefsAnalyzer` has no `impl RefsAnalyzer`

- [ ] **Step 3: Implement `TsRefsAnalyzer`**

Add the full implementation to `src/refs/ts.rs`:

```rust
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;
use super::RefsAnalyzer;

fn node_text<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Strip generic args: `UserService<T>` → `UserService` (take first type_identifier child of generic_type).
fn plain_type_name(node: Node, bytes: &[u8]) -> String {
    match node.kind() {
        "type_identifier" => node_text(node, bytes).to_string(),
        "generic_type" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "type_identifier")
                .map(|c| node_text(c, bytes).to_string())
                .unwrap_or_default()
        }
        _ => {
            // recurse one level to find type_identifier
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "type_identifier")
                .map(|c| node_text(c, bytes).to_string())
                .unwrap_or_default()
        }
    }
}

/// Phase 1: build var_name → type_name map.
fn build_type_map(root: Node, bytes: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    collect_type_map(root, bytes, &mut map);
    map
}

fn collect_type_map(node: Node, bytes: &[u8], map: &mut HashMap<String, String>) {
    match node.kind() {
        // const x: UserService = new UserService()
        "variable_declarator" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(n, bytes).to_string())
                .unwrap_or_default();
            if name.is_empty() { recurse(node, bytes, map); return; }

            // Try explicit type annotation: `const x: Type`
            // In TS grammar, variable_declarator may have a `type` field (type_annotation)
            let type_name = if let Some(ta) = node.child_by_field_name("type") {
                // type_annotation node wraps the actual type
                let mut cur = ta.walk();
                ta.children(&mut cur)
                    .filter(|c| c.kind() != ":" )
                    .find_map(|c| {
                        let t = plain_type_name(c, bytes);
                        if t.is_empty() { None } else { Some(t) }
                    })
                    .unwrap_or_default()
            } else {
                // No annotation — try new_expression RHS
                node.child_by_field_name("value")
                    .filter(|v| v.kind() == "new_expression")
                    .and_then(|v| v.child_by_field_name("constructor"))
                    .map(|c| node_text(c, bytes).to_string())
                    .unwrap_or_default()
            };

            if !type_name.is_empty() {
                map.insert(name, type_name);
            }
            recurse(node, bytes, map);
        }
        // function params: required_parameter / optional_parameter
        "required_parameter" | "optional_parameter" => {
            let name = node.child_by_field_name("pattern")
                .map(|n| node_text(n, bytes).to_string())
                .unwrap_or_default();
            if !name.is_empty() {
                if let Some(ta) = node.child_by_field_name("type") {
                    let mut cur = ta.walk();
                    let type_name = ta.children(&mut cur)
                        .filter(|c| c.kind() != ":")
                        .find_map(|c| {
                            let t = plain_type_name(c, bytes);
                            if t.is_empty() { None } else { Some(t) }
                        })
                        .unwrap_or_default();
                    if !type_name.is_empty() {
                        map.insert(name, type_name);
                    }
                }
            }
            recurse(node, bytes, map);
        }
        _ => recurse(node, bytes, map),
    }
}

fn recurse(node: Node, bytes: &[u8], map: &mut HashMap<String, String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_type_map(child, bytes, map);
    }
}

/// Phase 2: collect (type_name, Option<member>) candidates.
fn collect_candidates(
    node: Node,
    bytes: &[u8],
    type_map: &HashMap<String, String>,
    out: &mut Vec<(String, Option<String>)>,
) {
    match node.kind() {
        "import_statement" | "import_clause" | "import_specifier" => {
            // skip import subtrees
        }

        // new UserRepo()
        "new_expression" => {
            if let Some(ctor) = node.child_by_field_name("constructor") {
                let name = node_text(ctor, bytes).to_string();
                if !name.is_empty() {
                    out.push((name, None));
                }
            }
            recurse_candidates(node, bytes, type_map, out);
        }

        // svc.findUser(...) or repo.save(...)
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                if func.kind() == "member_expression" {
                    let obj = func.child_by_field_name("object")
                        .map(|n| node_text(n, bytes))
                        .unwrap_or("");
                    let prop = func.child_by_field_name("property")
                        .map(|n| node_text(n, bytes).to_string())
                        .unwrap_or_default();
                    if let Some(resolved) = type_map.get(obj) {
                        if !prop.is_empty() {
                            out.push((resolved.clone(), Some(prop)));
                        }
                    }
                }
            }
            recurse_candidates(node, bytes, type_map, out);
        }

        // x.field (outside call) — field/property access
        "member_expression" => {
            let obj = node.child_by_field_name("object")
                .map(|n| node_text(n, bytes))
                .unwrap_or("");
            let prop = node.child_by_field_name("property")
                .map(|n| node_text(n, bytes).to_string())
                .unwrap_or_default();
            if let Some(resolved) = type_map.get(obj) {
                if !prop.is_empty() {
                    out.push((resolved.clone(), Some(prop)));
                }
            }
            recurse_candidates(node, bytes, type_map, out);
        }

        // Type annotation positions: `: UserService`
        "type_annotation" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let t = plain_type_name(child, bytes);
                if !t.is_empty() {
                    out.push((t, None));
                }
            }
            // don't recurse further — type_annotation children are leaf-like
        }

        _ => recurse_candidates(node, bytes, type_map, out),
    }
}

fn recurse_candidates(
    node: Node,
    bytes: &[u8],
    type_map: &HashMap<String, String>,
    out: &mut Vec<(String, Option<String>)>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_candidates(child, bytes, type_map, out);
    }
}

impl RefsAnalyzer for TsRefsAnalyzer {
    fn extract_refs(
        &self,
        source: &str,
        import_map: &HashMap<String, String>,
        dep_sigs: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<String>> {
        // Try TypeScript grammar first, fall back to JavaScript
        let mut parser = tree_sitter::Parser::new();
        let lang_result = parser.set_language(
            &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        );
        if lang_result.is_err() {
            return HashMap::new();
        }

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

        // Phase 3: cross-reference
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

        refs.into_iter()
            .map(|(k, v)| { let mut names: Vec<String> = v.into_iter().collect(); names.sort(); (k, names) })
            .collect()
    }
}
```

Also add `pub mod ts;` to `src/refs/mod.rs` and the dispatch arm:

```rust
// in mod.rs
pub mod ts;

// in get_refs_analyzer:
Language::TypeScript | Language::JavaScript => Some(Box::new(ts::TsRefsAnalyzer::new())),
```

- [ ] **Step 4: Run tests**

```
cargo test -p codeskel refs::ts -- --nocapture
```
Expected: all 3 pass

- [ ] **Step 5: Commit**

```bash
git add src/refs/ts.rs src/refs/mod.rs
git commit -m "feat: add TypeScript/JavaScript RefsAnalyzer"
```

---

## Task 3: Python `RefsAnalyzer`

**Files:**
- Create: `src/refs/python.rs`
- Modify: `src/refs/mod.rs`

### What to implement

**Phase 1 — binding map:** Walk `typed_parameter` (function params with type), `assignment` with explicit type annotation (annotated assignment node in Python grammar), and plain assignment with constructor call RHS (best-effort).

**Phase 2 — candidates:** `call` with `attribute` func → resolve object → emit `(type, method)`; `call` with uppercase `identifier` func → emit `(identifier, None)` (constructor); `attribute` outside call → field access; `type` in function annotations → emit type names; `isinstance(x, Type)` → emit `(Type, None)`.

Skip `import_from_statement` subtrees.

- [ ] **Step 1: Write failing tests in `src/refs/python.rs`**

```rust
use std::collections::HashMap;
use super::RefsAnalyzer;

pub struct PythonRefsAnalyzer;

impl PythonRefsAnalyzer {
    pub fn new() -> Self { Self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_typed_param() {
        let source = r#"
from .models import User

def save_user(user: User) -> None:
    user.validate()
"#;
        let mut import_map = HashMap::new();
        import_map.insert("User".to_string(), "src/models.py".to_string());
        let mut dep_sigs = HashMap::new();
        dep_sigs.insert("src/models.py".to_string(),
            vec!["User".to_string(), "validate".to_string()]);

        let refs = PythonRefsAnalyzer::new().extract_refs(source, &import_map, &dep_sigs);
        let user_refs = refs.get("src/models.py").expect("models.py must appear");
        assert!(user_refs.contains(&"User".to_string()),
            "User type ref missing; got: {:?}", user_refs);
        assert!(user_refs.contains(&"validate".to_string()),
            "validate method ref missing; got: {:?}", user_refs);
    }

    #[test]
    fn test_python_constructor_call() {
        let source = r#"
from .repo import UserRepository

def main():
    repo = UserRepository()
    repo.find_by_id("1")
"#;
        let mut import_map = HashMap::new();
        import_map.insert("UserRepository".to_string(), "src/repo.py".to_string());
        let mut dep_sigs = HashMap::new();
        dep_sigs.insert("src/repo.py".to_string(),
            vec!["UserRepository".to_string(), "find_by_id".to_string()]);

        let refs = PythonRefsAnalyzer::new().extract_refs(source, &import_map, &dep_sigs);
        let repo_refs = refs.get("src/repo.py").expect("repo.py must appear");
        assert!(repo_refs.contains(&"UserRepository".to_string()),
            "constructor ref missing; got: {:?}", repo_refs);
        assert!(repo_refs.contains(&"find_by_id".to_string()),
            "find_by_id method ref missing; got: {:?}", repo_refs);
    }

    #[test]
    fn test_python_no_imports() {
        let source = "x = 1";
        let refs = PythonRefsAnalyzer::new().extract_refs(
            source, &HashMap::new(), &HashMap::new());
        assert!(refs.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test -p codeskel refs::python -- --nocapture 2>&1 | head -20
```
Expected: compile error — no `impl RefsAnalyzer`

- [ ] **Step 3: Implement `PythonRefsAnalyzer`**

```rust
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;
use super::RefsAnalyzer;

fn node_text<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Extract base type name, stripping subscripts: `List[User]` → `User`.
fn extract_type_name(node: Node, bytes: &[u8]) -> String {
    match node.kind() {
        "identifier" | "type" => node_text(node, bytes).to_string(),
        "subscript" => {
            // List[User] — take the first type_identifier inside
            let mut cur = node.walk();
            node.children(&mut cur)
                .find(|c| c.kind() == "identifier")
                .map(|c| node_text(c, bytes).to_string())
                .unwrap_or_default()
        }
        _ => {
            // Search children for an identifier
            let mut cur = node.walk();
            node.children(&mut cur)
                .find(|c| c.kind() == "identifier")
                .map(|c| node_text(c, bytes).to_string())
                .unwrap_or_default()
        }
    }
}

fn build_type_map(root: Node, bytes: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    collect_type_map(root, bytes, &mut map);
    map
}

fn collect_type_map(node: Node, bytes: &[u8], map: &mut HashMap<String, String>) {
    match node.kind() {
        // def foo(user: User, ...)
        "typed_parameter" | "typed_default_parameter" => {
            // children: identifier, ":", type
            let mut cur = node.walk();
            let children: Vec<Node> = node.children(&mut cur).collect();
            if let Some(name_node) = children.first() {
                if name_node.kind() == "identifier" {
                    let name = node_text(*name_node, bytes).to_string();
                    if name == "self" { recurse_type_map(node, bytes, map); return; }
                    // find the type node (after ":")
                    if let Some(type_node) = children.iter().skip(1).find(|c| c.kind() != ":") {
                        let t = extract_type_name(*type_node, bytes);
                        if !t.is_empty() { map.insert(name, t); }
                    }
                }
            }
            recurse_type_map(node, bytes, map);
        }
        // x: User = ...
        "annotated_assignment" => {
            if let Some(name_node) = node.child_by_field_name("target") {
                let name = node_text(name_node, bytes).to_string();
                if let Some(ann) = node.child_by_field_name("annotation") {
                    let t = extract_type_name(ann, bytes);
                    if !t.is_empty() { map.insert(name, t); }
                }
            }
            recurse_type_map(node, bytes, map);
        }
        // x = SomeClass()  — plain assignment best-effort
        "assignment" => {
            // lhs via "left", rhs via "right"
            if let (Some(lhs), Some(rhs)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) {
                if rhs.kind() == "call" {
                    // get the function node of the call
                    if let Some(func) = rhs.child_by_field_name("function") {
                        if func.kind() == "identifier" {
                            let ctor = node_text(func, bytes).to_string();
                            if ctor.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                                let var = node_text(lhs, bytes).to_string();
                                if !var.is_empty() { map.insert(var, ctor); }
                            }
                        }
                    }
                }
            }
            recurse_type_map(node, bytes, map);
        }
        _ => recurse_type_map(node, bytes, map),
    }
}

fn recurse_type_map(node: Node, bytes: &[u8], map: &mut HashMap<String, String>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        collect_type_map(child, bytes, map);
    }
}

fn collect_candidates(
    node: Node,
    bytes: &[u8],
    type_map: &HashMap<String, String>,
    out: &mut Vec<(String, Option<String>)>,
) {
    match node.kind() {
        "import_from_statement" | "import_statement" => {} // skip

        "call" => {
            if let Some(func) = node.child_by_field_name("function") {
                match func.kind() {
                    "attribute" => {
                        // obj.method(...)
                        let obj_text = func.child_by_field_name("object")
                            .map(|n| node_text(n, bytes))
                            .unwrap_or("");
                        let attr = func.child_by_field_name("attribute")
                            .map(|n| node_text(n, bytes).to_string())
                            .unwrap_or_default();
                        if let Some(resolved) = type_map.get(obj_text) {
                            if !attr.is_empty() {
                                out.push((resolved.clone(), Some(attr)));
                            }
                        }
                    }
                    "identifier" => {
                        // SomeClass() or isinstance(x, Type)
                        let name = node_text(func, bytes).to_string();
                        if name == "isinstance" {
                            // isinstance(x, Type) — second arg is the type
                            if let Some(args) = node.child_by_field_name("arguments") {
                                let mut cur = args.walk();
                                let type_arg = args.children(&mut cur)
                                    .filter(|c| c.kind() == "identifier")
                                    .nth(1); // second identifier
                                if let Some(t) = type_arg {
                                    let t_name = node_text(t, bytes).to_string();
                                    if !t_name.is_empty() { out.push((t_name, None)); }
                                }
                            }
                        } else if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                            out.push((name, None));
                        }
                    }
                    _ => {}
                }
            }
            recurse_candidates(node, bytes, type_map, out);
        }

        // obj.field outside a call
        "attribute" => {
            let obj_text = node.child_by_field_name("object")
                .map(|n| node_text(n, bytes))
                .unwrap_or("");
            let attr = node.child_by_field_name("attribute")
                .map(|n| node_text(n, bytes).to_string())
                .unwrap_or_default();
            if let Some(resolved) = type_map.get(obj_text) {
                if !attr.is_empty() {
                    out.push((resolved.clone(), Some(attr)));
                }
            }
            recurse_candidates(node, bytes, type_map, out);
        }

        // Return type annotations and other type positions
        "type" => {
            let t = node_text(node, bytes).to_string();
            if !t.is_empty() { out.push((t, None)); }
        }

        _ => recurse_candidates(node, bytes, type_map, out),
    }
}

fn recurse_candidates(
    node: Node,
    bytes: &[u8],
    type_map: &HashMap<String, String>,
    out: &mut Vec<(String, Option<String>)>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        collect_candidates(child, bytes, type_map, out);
    }
}

impl RefsAnalyzer for PythonRefsAnalyzer {
    fn extract_refs(
        &self,
        source: &str,
        import_map: &HashMap<String, String>,
        dep_sigs: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<String>> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into()).unwrap();
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return HashMap::new(),
        };
        let bytes = source.as_bytes();
        let root = tree.root_node();

        let type_map = build_type_map(root, bytes);
        let mut candidates: Vec<(String, Option<String>)> = Vec::new();
        collect_candidates(root, bytes, &type_map, &mut candidates);

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

        refs.into_iter()
            .map(|(k, v)| { let mut names: Vec<String> = v.into_iter().collect(); names.sort(); (k, names) })
            .collect()
    }
}
```

Add to `src/refs/mod.rs`:
- `pub mod python;`
- dispatch arm: `Language::Python => Some(Box::new(python::PythonRefsAnalyzer::new())),`

- [ ] **Step 4: Run tests**

```
cargo test -p codeskel refs::python -- --nocapture
```
Expected: all 3 pass

- [ ] **Step 5: Commit**

```bash
git add src/refs/python.rs src/refs/mod.rs
git commit -m "feat: add Python RefsAnalyzer"
```

---

## Task 4: Go `RefsAnalyzer`

**Files:**
- Create: `src/refs/go.rs`
- Modify: `src/refs/mod.rs`

### What to implement

**Phase 1 — binding map:** `short_var_declaration` (`:=`) where RHS is a call on a selector (e.g. `models.NewUser()`) → bind lhs to the package/type; `var_declaration` with explicit type; `parameter_declaration` with type (strip `*` pointer prefix).

**Phase 2 — candidates:** `selector_expression` — if left side is in import_map as a package, emit `(dep_path, right-side)` directly; if left side is in type_map, emit `(resolved_type, right-side)`; `type_identifier` in composite literals, type assertions.

Skip `import_declaration`.

- [ ] **Step 1: Write failing tests in `src/refs/go.rs`**

```rust
use std::collections::HashMap;
use super::RefsAnalyzer;

pub struct GoRefsAnalyzer;

impl GoRefsAnalyzer {
    pub fn new() -> Self { Self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_go_package_selector() {
        let source = r#"
package main

import "myapp/models"

func main() {
    u := models.NewUser()
    _ = u
}
"#;
        let mut import_map = HashMap::new();
        // package alias "models" → dep path
        import_map.insert("models".to_string(), "models/user.go".to_string());
        let mut dep_sigs = HashMap::new();
        dep_sigs.insert("models/user.go".to_string(),
            vec!["NewUser".to_string(), "User".to_string()]);

        let refs = GoRefsAnalyzer::new().extract_refs(source, &import_map, &dep_sigs);
        let model_refs = refs.get("models/user.go").expect("models/user.go must appear");
        assert!(model_refs.contains(&"NewUser".to_string()),
            "NewUser missing; got: {:?}", model_refs);
    }

    #[test]
    fn test_go_typed_param() {
        let source = r#"
package main

import "myapp/repo"

func Save(r *repo.UserRepo) {
    r.Insert()
}
"#;
        // In this case, the param type is qualified `repo.UserRepo`.
        // Phase 1 picks up the param type; Phase 2 catches method call on r.
        // Note: for qualified types like `repo.UserRepo`, we store the package alias "repo".
        let mut import_map = HashMap::new();
        import_map.insert("repo".to_string(), "repo/userrepo.go".to_string());
        let mut dep_sigs = HashMap::new();
        dep_sigs.insert("repo/userrepo.go".to_string(),
            vec!["UserRepo".to_string(), "Insert".to_string()]);

        let refs = GoRefsAnalyzer::new().extract_refs(source, &import_map, &dep_sigs);
        let repo_refs = refs.get("repo/userrepo.go").expect("repo/userrepo.go must appear");
        assert!(repo_refs.contains(&"Insert".to_string()),
            "Insert missing; got: {:?}", repo_refs);
    }

    #[test]
    fn test_go_no_imports() {
        let source = "package main\nfunc main() {}";
        let refs = GoRefsAnalyzer::new().extract_refs(
            source, &HashMap::new(), &HashMap::new());
        assert!(refs.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test -p codeskel refs::go -- --nocapture 2>&1 | head -20
```

- [ ] **Step 3: Implement `GoRefsAnalyzer`**

Go's import_map is keyed by **package alias** (last segment of import path, or explicit alias). The `build_import_map` in `get.rs` already uses sig names for Go, which means the caller has `import_map` keyed by sig names. However, Go's pattern is package-level (`models.NewUser()` uses `models` as the key, not `NewUser`).

**Important reconciliation:** For Go, `build_import_map` (sig-name–based) maps individual function/struct names to dep paths. The `GoRefsAnalyzer` will use these sig names directly in `selector_expression` phase 2 — when we see `models.NewUser()`, we emit `("NewUser", None)` as a candidate; the import_map's `"NewUser" → "models/user.go"` entry will match. For package-level bindings (variable `u` bound to a call on `models.NewUser`), phase 1 tracks `u → "models"` (the package alias). Phase 2 then resolves `u.SomeMethod()` by looking up `"models"` in import_map — this won't match individual sig names. 

**Simpler approach for Go (consistent with PRD):** Since `build_import_map` for Go uses sig names, focus Phase 2 on `selector_expression` where the left side is a **variable** in the binding map (which stores the package alias). To resolve package aliases: maintain a secondary package_map derived from `import_map`'s values — for any `(sig_name → dep_path)` entry, the package alias is the dep path's parent directory stem. Or more practically: when we see `models.NewUser()`, emit `("NewUser", None)` — if `NewUser` is in `import_map` directly, it resolves. This avoids needing package alias resolution entirely for Phase 2 direct selectors.

```rust
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;
use super::RefsAnalyzer;

fn node_text<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Strip pointer prefix: `*models.UserRepo` → take package "models"
fn strip_pointer(s: &str) -> &str {
    s.trim_start_matches('*')
}

fn build_type_map(root: Node, bytes: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    collect_type_map(root, bytes, &mut map);
    map
}

fn collect_type_map(node: Node, bytes: &[u8], map: &mut HashMap<String, String>) {
    match node.kind() {
        "import_declaration" | "import_spec_list" | "import_spec" => {}

        // var x SomeType
        "var_declaration" | "var_spec" => {
            // var_spec: name, type, value
            if node.kind() == "var_spec" {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, bytes).to_string();
                    if let Some(type_node) = node.child_by_field_name("type") {
                        let t = extract_go_type(type_node, bytes);
                        if !t.is_empty() { map.insert(name, t); }
                    }
                }
            }
            recurse_type_map(node, bytes, map);
        }

        // func foo(r *repo.UserRepo)
        "parameter_declaration" => {
            // names may be multiple; type is the last child that's a type node
            if let Some(type_node) = node.child_by_field_name("type") {
                let t = extract_go_type(type_node, bytes);
                if !t.is_empty() {
                    // all name children
                    let mut cur = node.walk();
                    for child in node.children(&mut cur) {
                        if child.kind() == "identifier" {
                            let name = node_text(child, bytes).to_string();
                            map.insert(name, t.clone());
                        }
                    }
                }
            }
            recurse_type_map(node, bytes, map);
        }

        // u := models.NewUser()
        "short_var_declaration" => {
            // left: expression_list, right: expression_list
            if let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) {
                // Get first name on left
                let lhs_name = {
                    let mut cur = left.walk();
                    left.children(&mut cur)
                        .find(|c| c.kind() == "identifier")
                        .map(|c| node_text(c, bytes).to_string())
                        .unwrap_or_default()
                };
                // Check if rhs is a call on a selector
                let mut cur = right.walk();
                let rhs = right.children(&mut cur).next();
                if let Some(rhs_node) = rhs {
                    if rhs_node.kind() == "call_expression" {
                        if let Some(func) = rhs_node.child_by_field_name("function") {
                            if func.kind() == "selector_expression" {
                                let pkg = func.child_by_field_name("operand")
                                    .map(|n| node_text(n, bytes).to_string())
                                    .unwrap_or_default();
                                if !lhs_name.is_empty() && !pkg.is_empty() {
                                    map.insert(lhs_name, pkg);
                                }
                            }
                        }
                    }
                }
            }
            recurse_type_map(node, bytes, map);
        }

        _ => recurse_type_map(node, bytes, map),
    }
}

/// Extract the package alias from a Go type: `*repo.UserRepo` → `repo`, `UserRepo` → `UserRepo`.
fn extract_go_type(node: Node, bytes: &[u8]) -> String {
    let text = node_text(node, bytes);
    let stripped = strip_pointer(text);
    // If qualified (pkg.Type), take the package alias
    if let Some(dot_pos) = stripped.find('.') {
        stripped[..dot_pos].to_string()
    } else {
        stripped.to_string()
    }
}

fn recurse_type_map(node: Node, bytes: &[u8], map: &mut HashMap<String, String>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        collect_type_map(child, bytes, map);
    }
}

fn collect_candidates(
    node: Node,
    bytes: &[u8],
    type_map: &HashMap<String, String>,
    import_map: &HashMap<String, String>,
    out: &mut Vec<(String, Option<String>)>,
) {
    match node.kind() {
        "import_declaration" | "import_spec_list" | "import_spec" => {}

        // models.NewUser() or u.Method()
        "selector_expression" => {
            let operand = node.child_by_field_name("operand")
                .map(|n| node_text(n, bytes))
                .unwrap_or("");
            let field = node.child_by_field_name("field")
                .map(|n| node_text(n, bytes).to_string())
                .unwrap_or_default();

            if field.is_empty() {
                recurse_candidates(node, bytes, type_map, import_map, out);
                return;
            }

            // Case 1: operand is a package alias in import_map (direct package call like models.NewUser)
            // Since import_map is keyed by sig names for Go (from build_import_map),
            // check if field is directly in import_map.
            if import_map.contains_key(&field) {
                out.push((field.clone(), None));
            }

            // Case 2: operand is a variable in type_map (instance method call)
            if let Some(pkg_or_type) = type_map.get(operand) {
                // pkg_or_type is either a package alias or a type name
                // Try as a direct type name in import_map
                if import_map.contains_key(pkg_or_type.as_str()) {
                    out.push((pkg_or_type.clone(), Some(field.clone())));
                } else {
                    // Try as package alias: check if any import_map key maps to a dep whose path
                    // contains this package alias. Emit (field, None) as a fallback — Phase 3
                    // will only match if field is in import_map.
                    out.push((field.clone(), None));
                }
            }

            recurse_candidates(node, bytes, type_map, import_map, out);
        }

        _ => recurse_candidates(node, bytes, type_map, import_map, out),
    }
}

fn recurse_candidates(
    node: Node,
    bytes: &[u8],
    type_map: &HashMap<String, String>,
    import_map: &HashMap<String, String>,
    out: &mut Vec<(String, Option<String>)>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        collect_candidates(child, bytes, type_map, import_map, out);
    }
}

impl RefsAnalyzer for GoRefsAnalyzer {
    fn extract_refs(
        &self,
        source: &str,
        import_map: &HashMap<String, String>,
        dep_sigs: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<String>> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_go::LANGUAGE.into()).unwrap();
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return HashMap::new(),
        };
        let bytes = source.as_bytes();
        let root = tree.root_node();

        let type_map = build_type_map(root, bytes);
        let mut candidates: Vec<(String, Option<String>)> = Vec::new();
        collect_candidates(root, bytes, &type_map, import_map, &mut candidates);

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

        refs.into_iter()
            .map(|(k, v)| { let mut names: Vec<String> = v.into_iter().collect(); names.sort(); (k, names) })
            .collect()
    }
}
```

Add to `src/refs/mod.rs`:
- `pub mod go;`
- dispatch arm: `Language::Go => Some(Box::new(go::GoRefsAnalyzer::new())),`

- [ ] **Step 4: Run tests**

```
cargo test -p codeskel refs::go -- --nocapture
```
Expected: all 3 pass

- [ ] **Step 5: Commit**

```bash
git add src/refs/go.rs src/refs/mod.rs
git commit -m "feat: add Go RefsAnalyzer"
```

---

## Task 5: Rust `RefsAnalyzer`

**Files:**
- Create: `src/refs/rust.rs`
- Modify: `src/refs/mod.rs`

### What to implement

**Phase 1 — binding map:** `let_declaration` with explicit type or `call_expression` RHS of known type (`TypeName::new()`); `parameter` with type (strip `&`, `&mut`, lifetimes).

**Phase 2 — candidates:** `field_expression` → resolve → emit `(type, field)`; `method_call_expression` → resolve → emit `(type, method)`; `call_expression` with `scoped_identifier` function (`TypeName::method()`) → emit `(TypeName, method)` and `(TypeName, None)`; `struct_expression` → emit `(TypeName, None)`; `type_identifier` in type positions.

Skip `use_declaration`.

- [ ] **Step 1: Write failing tests in `src/refs/rust.rs`**

```rust
use std::collections::HashMap;
use super::RefsAnalyzer;

pub struct RustRefsAnalyzer;

impl RustRefsAnalyzer {
    pub fn new() -> Self { Self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_typed_param_method_call() {
        let source = r#"
use crate::models::User;
use crate::repo::UserRepo;

fn save(repo: &UserRepo, user: User) {
    repo.insert(&user);
}
"#;
        let mut import_map = HashMap::new();
        import_map.insert("UserRepo".to_string(), "src/repo.rs".to_string());
        import_map.insert("User".to_string(), "src/models.rs".to_string());
        let mut dep_sigs = HashMap::new();
        dep_sigs.insert("src/repo.rs".to_string(),
            vec!["UserRepo".to_string(), "insert".to_string()]);
        dep_sigs.insert("src/models.rs".to_string(),
            vec!["User".to_string()]);

        let refs = RustRefsAnalyzer::new().extract_refs(source, &import_map, &dep_sigs);
        let repo_refs = refs.get("src/repo.rs").expect("repo.rs must appear");
        assert!(repo_refs.contains(&"insert".to_string()),
            "insert missing; got: {:?}", repo_refs);
        assert!(repo_refs.contains(&"UserRepo".to_string()),
            "UserRepo type ref missing; got: {:?}", repo_refs);
    }

    #[test]
    fn test_rust_scoped_call() {
        let source = r#"
use crate::models::User;

fn main() {
    let u = User::new("alice");
}
"#;
        let mut import_map = HashMap::new();
        import_map.insert("User".to_string(), "src/models.rs".to_string());
        let mut dep_sigs = HashMap::new();
        dep_sigs.insert("src/models.rs".to_string(),
            vec!["User".to_string(), "new".to_string()]);

        let refs = RustRefsAnalyzer::new().extract_refs(source, &import_map, &dep_sigs);
        let model_refs = refs.get("src/models.rs").expect("models.rs must appear");
        assert!(model_refs.contains(&"User".to_string()),
            "User missing; got: {:?}", model_refs);
        assert!(model_refs.contains(&"new".to_string()),
            "new missing; got: {:?}", model_refs);
    }

    #[test]
    fn test_rust_no_imports() {
        let source = "fn main() { let x = 1; }";
        let refs = RustRefsAnalyzer::new().extract_refs(
            source, &HashMap::new(), &HashMap::new());
        assert!(refs.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test -p codeskel refs::rust -- --nocapture 2>&1 | head -20
```

- [ ] **Step 3: Implement `RustRefsAnalyzer`**

```rust
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;
use super::RefsAnalyzer;

fn node_text<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or("")
}

/// Strip reference/lifetime prefixes: `&'a UserService` → `UserService`, `&mut Repo` → `Repo`
fn strip_ref_prefix(s: &str) -> &str {
    let s = s.trim_start_matches('&').trim();
    let s = s.trim_start_matches("mut").trim();
    // strip lifetime: `'a Type` → `Type`
    if s.starts_with('\'') {
        if let Some(space) = s.find(' ') {
            return s[space..].trim();
        }
    }
    s
}

fn build_type_map(root: Node, bytes: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    collect_type_map(root, bytes, &mut map);
    map
}

fn collect_type_map(node: Node, bytes: &[u8], map: &mut HashMap<String, String>) {
    match node.kind() {
        "use_declaration" => {} // skip

        // fn foo(repo: &UserRepo, user: User)
        "parameter" => {
            let name = node.child_by_field_name("pattern")
                .map(|n| node_text(n, bytes).to_string())
                .unwrap_or_default();
            if name == "self" || name == "_" || name.is_empty() {
                recurse_type_map(node, bytes, map);
                return;
            }
            if let Some(type_node) = node.child_by_field_name("type") {
                let raw = node_text(type_node, bytes);
                let t = strip_ref_prefix(raw).to_string();
                // Handle qualified paths: keep just the last segment
                let t = t.split("::").last().unwrap_or(&t).trim().to_string();
                if !t.is_empty() { map.insert(name, t); }
            }
            recurse_type_map(node, bytes, map);
        }

        // let x: UserService = ...
        "let_declaration" => {
            let name = node.child_by_field_name("pattern")
                .map(|n| node_text(n, bytes).to_string())
                .unwrap_or_default();
            if !name.is_empty() {
                // Explicit type annotation
                if let Some(type_node) = node.child_by_field_name("type") {
                    let raw = node_text(type_node, bytes);
                    let t = strip_ref_prefix(raw).split("::").last()
                        .unwrap_or(raw).trim().to_string();
                    if !t.is_empty() { map.insert(name.clone(), t); }
                } else if let Some(val) = node.child_by_field_name("value") {
                    // let x = UserService::new() — scoped_identifier function
                    if val.kind() == "call_expression" {
                        if let Some(func) = val.child_by_field_name("function") {
                            let func_text = node_text(func, bytes);
                            // TypeName::method — take the part before ::
                            if let Some(type_part) = func_text.split("::").next() {
                                let t = type_part.trim().to_string();
                                if !t.is_empty() && t.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                                    map.insert(name, t);
                                }
                            }
                        }
                    }
                }
            }
            recurse_type_map(node, bytes, map);
        }

        _ => recurse_type_map(node, bytes, map),
    }
}

fn recurse_type_map(node: Node, bytes: &[u8], map: &mut HashMap<String, String>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        collect_type_map(child, bytes, map);
    }
}

fn collect_candidates(
    node: Node,
    bytes: &[u8],
    type_map: &HashMap<String, String>,
    out: &mut Vec<(String, Option<String>)>,
) {
    match node.kind() {
        "use_declaration" => {} // skip

        // repo.insert(&user)
        "method_call_expression" => {
            let receiver = node.child_by_field_name("receiver")
                .map(|n| node_text(n, bytes))
                .unwrap_or("");
            let method = node.child_by_field_name("name")
                .map(|n| node_text(n, bytes).to_string())
                .unwrap_or_default();
            if let Some(resolved) = type_map.get(receiver) {
                if !method.is_empty() {
                    out.push((resolved.clone(), Some(method)));
                }
            }
            recurse_candidates(node, bytes, type_map, out);
        }

        // user.name (field access)
        "field_expression" => {
            let value = node.child_by_field_name("value")
                .map(|n| node_text(n, bytes))
                .unwrap_or("");
            let field = node.child_by_field_name("field")
                .map(|n| node_text(n, bytes).to_string())
                .unwrap_or_default();
            if let Some(resolved) = type_map.get(value) {
                if !field.is_empty() {
                    out.push((resolved.clone(), Some(field)));
                }
            }
            recurse_candidates(node, bytes, type_map, out);
        }

        // User::new(...) or UserRepo::find(...)
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                let func_text = node_text(func, bytes);
                if func_text.contains("::") {
                    let parts: Vec<&str> = func_text.split("::").collect();
                    if parts.len() >= 2 {
                        let type_name = parts[parts.len() - 2].trim().to_string();
                        let method_name = parts[parts.len() - 1].trim().to_string();
                        if !type_name.is_empty() {
                            out.push((type_name.clone(), None));
                            if !method_name.is_empty() {
                                out.push((type_name, Some(method_name)));
                            }
                        }
                    }
                }
            }
            recurse_candidates(node, bytes, type_map, out);
        }

        // User { name: "alice", .. }
        "struct_expression" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, bytes).to_string();
                if !name.is_empty() {
                    out.push((name, None));
                }
            }
            recurse_candidates(node, bytes, type_map, out);
        }

        // type_identifier in type annotation positions
        "type_identifier" => {
            let name = node_text(node, bytes).to_string();
            if !name.is_empty() {
                out.push((name, None));
            }
        }

        _ => recurse_candidates(node, bytes, type_map, out),
    }
}

fn recurse_candidates(
    node: Node,
    bytes: &[u8],
    type_map: &HashMap<String, String>,
    out: &mut Vec<(String, Option<String>)>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        collect_candidates(child, bytes, type_map, out);
    }
}

impl RefsAnalyzer for RustRefsAnalyzer {
    fn extract_refs(
        &self,
        source: &str,
        import_map: &HashMap<String, String>,
        dep_sigs: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<String>> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return HashMap::new(),
        };
        let bytes = source.as_bytes();
        let root = tree.root_node();

        let type_map = build_type_map(root, bytes);
        let mut candidates: Vec<(String, Option<String>)> = Vec::new();
        collect_candidates(root, bytes, &type_map, &mut candidates);

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

        refs.into_iter()
            .map(|(k, v)| { let mut names: Vec<String> = v.into_iter().collect(); names.sort(); (k, names) })
            .collect()
    }
}
```

Add to `src/refs/mod.rs`:
- `pub mod rust_refs;` (use `rust_refs` to avoid collision with the `rust` keyword)
- dispatch arm: `Language::Rust => Some(Box::new(rust_refs::RustRefsAnalyzer::new())),`

**Note:** The file can also be named `src/refs/rust_lang.rs` to mirror the pattern in `src/parsers/rust_lang.rs`. Use `rust_lang` to avoid the Rust keyword conflict.

- [ ] **Step 4: Run tests**

```
cargo test -p codeskel refs::rust_lang -- --nocapture
```
Expected: all 3 pass

- [ ] **Step 5: Commit**

```bash
git add src/refs/rust_lang.rs src/refs/mod.rs
git commit -m "feat: add Rust RefsAnalyzer"
```

---

## Task 6: Full test suite + integration smoke test

- [ ] **Step 1: Run full test suite**

```
cargo test -p codeskel 2>&1 | tail -20
```
Expected: all pass, 0 failures

- [ ] **Step 2: Build release binary**

```
cargo build --release 2>&1 | tail -5
```
Expected: Finished release

- [ ] **Step 3: Smoke test with existing Java fixture (regression check)**

```
./target/release/codeskel get --refs tests/fixtures/java_project/src/com/example/service/UserService.java --cache tests/fixtures/java_project/.codeskel/cache.json 2>&1
```
Expected: JSON with `"refs"` containing dep file → symbol names (not empty)

- [ ] **Step 4: Final commit if any cleanup needed**

```bash
git add -p
git commit -m "test: verify multi-language refs smoke test"
```
