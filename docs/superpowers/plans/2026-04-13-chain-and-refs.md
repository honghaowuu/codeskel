# `--chain` and `--refs` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `--chain` (transitive dep chain navigation) and `--refs` (Java symbol reference extraction) to `codeskel get`.

**Architecture:** `--chain` is a pure in-memory BFS over the existing cache graph, filtering `cache.order` (already topo-sorted, leaves/files-with-no-deps first) for the result. **Ordering note:** "leaves first" means files that have no dependencies come at index 0; the callers come last. This matches `cache.order`'s own ordering exactly. Do NOT reverse the filtered slice. `--refs` introduces a new `RefsAnalyzer` trait in `src/refs/mod.rs` mirroring the existing `LanguageParser` trait, with a Java implementation using tree-sitter in `src/refs/java.rs`. The `get` command dispatcher is refactored to check `--chain` and `--refs` before falling through to existing modes.

**Tech Stack:** Rust, tree-sitter-java (already a dependency), serde_json, anyhow, tempfile (dev)

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `tests/fixtures/java_refs_project/src/com/example/model/User.java` | Create | Test fixture: dep with public methods |
| `tests/fixtures/java_refs_project/src/com/example/repo/UserRepository.java` | Create | Test fixture: dep with public methods, depends on User |
| `tests/fixtures/java_refs_project/src/com/example/service/UserService.java` | Create | Test fixture: caller using both deps via typed local vars |
| `src/cli.rs` | Modify | Add `chain: Option<String>` and `refs: Option<String>` to `GetArgs` |
| `src/commands/get.rs` | Modify | Add dispatcher, `get_chain_count`, `get_chain_entry`, `get_refs` |
| `src/refs/mod.rs` | Create | `RefsAnalyzer` trait + `get_refs_analyzer` dispatch |
| `src/refs/java.rs` | Create | `JavaRefsAnalyzer`: three-phase tree-sitter walk |
| `src/lib.rs` | Modify | Add `pub mod refs` |

---

## Task 1: Create Java refs test fixture

**Files:**
- Create: `tests/fixtures/java_refs_project/src/com/example/model/User.java`
- Create: `tests/fixtures/java_refs_project/src/com/example/repo/UserRepository.java`
- Create: `tests/fixtures/java_refs_project/src/com/example/service/UserService.java`

No docstrings so all three files land in `cache.order` (to_comment) when scanned with min_coverage=0.0.

- [ ] **Step 1: Create User.java**

```java
package com.example.model;

public class User {
    private String email;

    public String getEmail() { return email; }
    public void setEmail(String email) { this.email = email; }
}
```

Save to `tests/fixtures/java_refs_project/src/com/example/model/User.java`.

- [ ] **Step 2: Create UserRepository.java**

```java
package com.example.repo;
import com.example.model.User;

public class UserRepository {
    public User findById(String id) { return null; }
    public void save(User user) {}
}
```

Save to `tests/fixtures/java_refs_project/src/com/example/repo/UserRepository.java`.

- [ ] **Step 3: Create UserService.java**

```java
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
```

Save to `tests/fixtures/java_refs_project/src/com/example/service/UserService.java`.

- [ ] **Step 4: Verify fixture scans correctly**

Run:
```bash
cargo test test_scan_java_fixture -- --nocapture 2>&1 | head -20
```

Then do a quick manual scan to confirm topo order:
```bash
cargo run -- scan tests/fixtures/java_refs_project --cache-dir /tmp/codeskel_test 2>&1
cat /tmp/codeskel_test/cache.json | python3 -m json.tool | grep '"order"' -A 10
```

Expected: `order` contains all three files with `User.java` first (leaf — no deps), `UserRepository.java` second (depends on User), `UserService.java` third (depends on both). If the order is wrong or files are missing from `order`, the resolver is not matching the imports to internal files — check that the package-to-path resolution works correctly for this fixture before continuing.

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/java_refs_project/
git commit -m "test: add java_refs_project fixture for chain and refs tests"
```

---

## Task 2: Add `--chain` and `--refs` to `GetArgs`

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Add the two new fields to `GetArgs`**

In `src/cli.rs`, add after the `deps` field:

```rust
/// Return transitive dep chain count, or with --index fetch one dep entry
#[arg(long, value_name = "FILE")]
pub chain: Option<String>,

/// Return symbol references from FILE's body to its internal deps (Java only)
#[arg(long, value_name = "FILE")]
pub refs: Option<String>,
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build 2>&1
```

Expected: compiles with zero errors. Warnings about unused fields are fine at this stage.

- [ ] **Step 3: Commit**

```bash
git add src/cli.rs
git commit -m "feat: add --chain and --refs flags to GetArgs"
```

---

## Task 3: Implement `--chain` (TDD)

**Files:**
- Modify: `src/commands/get.rs`
- Test: `tests/integration_scan.rs`

- [ ] **Step 1: Write failing integration test for `get_chain_count`**

Add to `tests/integration_scan.rs`:

```rust
#[test]
fn test_chain_count_for_userservice() {
    use codeskel::cache::read_cache;
    use codeskel::cli::GetArgs;

    let tmp = tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_refs_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache = read_cache(&tmp.path().join("cache.json")).unwrap();

    let svc_path = cache.files.keys()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in cache");

    // BFS from UserService: deps are User and UserRepository → count = 2
    // (cache.order only contains non-skipped files; min_coverage=0.0 means all 3 are in order)
    // NOTE: skipped files (high comment coverage) are excluded from cache.order and
    // therefore from the chain result. Here min_coverage=0.0 ensures all files are included.
    let chain = codeskel::commands::get::chain_order(&cache, &svc_path).unwrap();
    assert_eq!(chain.len(), 2, "UserService has 2 transitive deps; got: {:?}", chain);

    // Leaves-first order: User.java has no deps → index 0.
    // UserRepository.java depends on User → index 1.
    // Do NOT expect "deepest dep first" — "leaves first" is the correct description.
    assert!(chain[0].contains("User") && !chain[0].contains("UserRepository"),
        "index 0 should be User.java (leaf, no deps), got: {}", chain[0]);
    assert!(chain[1].contains("UserRepository"),
        "index 1 should be UserRepository.java, got: {}", chain[1]);
}

#[test]
fn test_chain_count_zero_for_leaf() {
    use codeskel::cache::read_cache;

    let tmp = tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_refs_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache = read_cache(&tmp.path().join("cache.json")).unwrap();

    let user_path = cache.files.keys()
        .find(|p| p.ends_with("User.java"))
        .cloned()
        .expect("User.java must be in cache");

    let chain = codeskel::commands::get::chain_order(&cache, &user_path).unwrap();
    assert_eq!(chain.len(), 0, "User.java has no deps");
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test test_chain_count -- --nocapture 2>&1
```

Expected: compile error — `chain_order` does not exist yet.

- [ ] **Step 3: Implement `chain_order`, `get_chain_count`, `get_chain_entry` in `get.rs`**

Add these imports at the top of `src/commands/get.rs`:
```rust
use std::collections::{HashSet, VecDeque};
```

Add the helper and the two command functions:

```rust
/// Returns the transitive dependency list for `file_path`, in the same order as `cache.order`
/// (leaves/files-with-no-deps first). `file_path` itself is excluded from the result.
///
/// Only includes files present in `cache.order` (non-skipped files). Transitive deps that were
/// skipped by the scanner (e.g., already well-commented) are silently excluded — this matches
/// `cache.order` semantics and means `count` reflects only the files that need commenting.
pub fn chain_order(cache: &crate::models::CacheFile, file_path: &str) -> anyhow::Result<Vec<String>> {
    let entry = cache.files.get(file_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found in cache", file_path))?;

    // BFS starting from file_path's imports — file_path itself is excluded
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    for dep in &entry.internal_imports {
        if visited.insert(dep.clone()) {
            queue.push_back(dep.clone());
        }
    }
    while let Some(current) = queue.pop_front() {
        if let Some(dep_entry) = cache.files.get(&current) {
            for dep in &dep_entry.internal_imports {
                if visited.insert(dep.clone()) {
                    queue.push_back(dep.clone());
                }
            }
        }
    }

    // Filter cache.order (already topo-sorted, leaves first) to visited set
    let chain: Vec<String> = cache.order.iter()
        .filter(|p| visited.contains(*p))
        .cloned()
        .collect();

    Ok(chain)
}

fn get_chain_count(cache: &crate::models::CacheFile, file_path: &str) -> anyhow::Result<bool> {
    let chain = chain_order(cache, file_path)?;
    let output = json!({ "for": file_path, "count": chain.len() });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(false)
}

fn get_chain_entry(cache: &crate::models::CacheFile, file_path: &str, index: usize) -> anyhow::Result<bool> {
    let chain = chain_order(cache, file_path)?;
    let dep_path = chain.get(index)
        .ok_or_else(|| anyhow::anyhow!(
            "Index {} out of range (chain has {} entries for '{}')",
            index, chain.len(), file_path
        ))?;
    // dep_path came from cache.order, so it is guaranteed to be in cache.files
    let entry = cache.files.get(dep_path)
        .expect("dep from cache.order must exist in cache.files");
    println!("{}", serde_json::to_string_pretty(entry)?);
    Ok(false)
}
```

- [ ] **Step 4: Refactor `run()` dispatcher in `get.rs`**

Replace the existing `run` function with:

```rust
pub fn run(args: GetArgs) -> anyhow::Result<bool> {
    let cache = read_cache(&args.cache_path)?;

    // --chain (with optional --index modifier)
    if let Some(chain_path) = &args.chain {
        if args.path.is_some() || args.deps.is_some() || args.refs.is_some() {
            anyhow::bail!("--chain cannot be combined with --path, --deps, or --refs");
        }
        return if let Some(idx) = args.index {
            get_chain_entry(&cache, chain_path, idx)
        } else {
            get_chain_count(&cache, chain_path)
        };
    }

    // --refs
    if let Some(refs_path) = &args.refs {
        if args.index.is_some() || args.path.is_some() || args.deps.is_some() {
            anyhow::bail!("--refs cannot be combined with --index, --path, or --deps");
        }
        return get_refs(&cache, refs_path);
    }

    // Existing: --index, --path, --deps
    let mode_count = args.index.is_some() as u8
        + args.path.is_some() as u8
        + args.deps.is_some() as u8;

    if mode_count == 0 {
        anyhow::bail!("One of --index, --path, --deps, --chain, or --refs is required");
    }
    if mode_count > 1 {
        anyhow::bail!("Only one of --index, --path, or --deps may be used at a time");
    }

    if let Some(deps_path) = &args.deps {
        return get_deps(&cache, deps_path);
    }

    let entry = if let Some(idx) = args.index {
        let rel = cache.order.get(idx).ok_or_else(|| {
            anyhow::anyhow!(
                "Index {} out of range (cache has {} items in order)",
                idx,
                cache.order.len()
            )
        })?;
        cache.files.get(rel).ok_or_else(|| anyhow::anyhow!("File {} not in cache", rel))?
    } else {
        let path = args.path.as_ref().unwrap();
        cache.files.get(path)
            .ok_or_else(|| anyhow::anyhow!("Path '{}' not found in cache", path))?
    };

    println!("{}", serde_json::to_string_pretty(entry)?);
    Ok(false)
}
```

Add a stub for `get_refs` so it compiles (implement in Task 6):
```rust
fn get_refs(_cache: &crate::models::CacheFile, _file_path: &str) -> anyhow::Result<bool> {
    anyhow::bail!("--refs not yet implemented")
}
```

- [ ] **Step 5: Run tests — chain tests should pass**

```bash
cargo test test_chain_count -- --nocapture 2>&1
```

Expected: both `test_chain_count_for_userservice` and `test_chain_count_zero_for_leaf` pass.

- [ ] **Step 6: Run full test suite to check no regressions**

```bash
cargo test 2>&1
```

Expected: all existing tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/commands/get.rs tests/integration_scan.rs
git commit -m "feat: implement --chain (get_chain_count + get_chain_entry) on codeskel get"
```

---

## Task 4: Create `src/refs` module scaffold

**Files:**
- Create: `src/refs/mod.rs`
- Create: `src/refs/java.rs` (stub)
- Modify: `src/lib.rs`

- [ ] **Step 1: Create `src/refs/mod.rs` with the trait**

```rust
pub mod java;

use std::collections::HashMap;
use crate::models::Language;

/// Analyzes a source file's body and returns, for each internal dep file,
/// the set of symbol names (from that dep's signatures) actually referenced.
pub trait RefsAnalyzer: Send + Sync {
    /// - `source`: raw source text of the file being analyzed
    /// - `import_map`: simple_name → dep_file_path (internal deps only)
    /// - `dep_sigs`: dep_file_path → list of signature names (all kinds)
    /// Returns: dep_file_path → sorted list of referenced symbol names
    fn extract_refs(
        &self,
        source: &str,
        import_map: &HashMap<String, String>,
        dep_sigs: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<String>>;
}

/// Returns a RefsAnalyzer for the given language, or None if unsupported.
pub fn get_refs_analyzer(lang: &Language) -> Option<Box<dyn RefsAnalyzer>> {
    match lang {
        Language::Java => Some(Box::new(java::JavaRefsAnalyzer::new())),
        _ => None,
    }
}
```

- [ ] **Step 2: Create stub `src/refs/java.rs`**

```rust
use std::collections::HashMap;
use super::RefsAnalyzer;

pub struct JavaRefsAnalyzer;

impl JavaRefsAnalyzer {
    pub fn new() -> Self { Self }
}

impl RefsAnalyzer for JavaRefsAnalyzer {
    fn extract_refs(
        &self,
        _source: &str,
        _import_map: &HashMap<String, String>,
        _dep_sigs: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, Vec<String>> {
        HashMap::new() // stub
    }
}
```

- [ ] **Step 3: Add `pub mod refs` to `src/lib.rs`**

Add after the existing mod declarations:
```rust
pub mod refs;
```

- [ ] **Step 4: Compile check**

```bash
cargo build 2>&1
```

Expected: compiles cleanly.

- [ ] **Step 5: Commit**

```bash
git add src/refs/ src/lib.rs
git commit -m "feat: add refs module scaffold (RefsAnalyzer trait + Java stub)"
```

---

## Task 5: Implement `JavaRefsAnalyzer` — Phase 1 (type map)

**Files:**
- Modify: `src/refs/java.rs`

- [ ] **Step 1: Write failing unit test for `build_type_map`**

Add to `src/refs/java.rs`:

```rust
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
        assert_eq!(map.get("user").map(String::as_str), Some("User"));
        assert_eq!(map.get("repo").map(String::as_str), Some("UserRepository"));
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
        // String is captured too (stdlib, will fail cross-ref step later)
        assert_eq!(map.get("name").map(String::as_str), Some("String"));
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test test_build_type_map -- --nocapture 2>&1
```

Expected: compile error — `build_type_map` does not exist.

- [ ] **Step 3: Implement `build_type_map` in `src/refs/java.rs`**

Replace the stub content with the full file:

```rust
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
fn type_name(node: Node, bytes: &[u8]) -> String {
    match node.kind() {
        "type_identifier" => node_text(node, bytes).to_string(),
        "generic_type" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "type_identifier")
                .map(|c| node_text(c, bytes).to_string())
                .unwrap_or_default()
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
                let t = type_name(type_node, bytes);
                if !t.is_empty() {
                    // Each variable_declarator child holds the name
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
                let t = type_name(type_node, bytes);
                if !t.is_empty() {
                    // Name is inside variable_declarator_id child
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
        HashMap::new() // stub — Phase 2+3 in next task
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
        assert_eq!(map.get("user").map(String::as_str), Some("User"));
        assert_eq!(map.get("repo").map(String::as_str), Some("UserRepository"));
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
```

- [ ] **Step 4: Run type map tests**

```bash
cargo test test_build_type_map -- --nocapture 2>&1
```

Expected: all 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/refs/java.rs
git commit -m "feat: implement JavaRefsAnalyzer Phase 1 (type map)"
```

---

## Task 6: Implement `JavaRefsAnalyzer` — Phase 2+3 (candidates + cross-reference)

**Files:**
- Modify: `src/refs/java.rs`

- [ ] **Step 1: Write the failing full `extract_refs` unit test**

Add to the `tests` module in `src/refs/java.rs`:

```rust
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
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test test_extract_refs -- --nocapture 2>&1
```

Expected: tests fail (extract_refs returns empty HashMap).

- [ ] **Step 3: Implement Phase 2 — candidate collection**

Add these functions to `src/refs/java.rs` (before the `impl RefsAnalyzer` block):

```rust
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
                let name = type_name(t, bytes);
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
```

- [ ] **Step 4: Implement Phase 3 — cross-reference, and wire into `extract_refs`**

Replace the stub `impl RefsAnalyzer for JavaRefsAnalyzer`:

```rust
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
```

- [ ] **Step 5: Run all refs unit tests**

```bash
cargo test test_extract_refs -- --nocapture 2>&1
cargo test test_build_type_map -- --nocapture 2>&1
```

Expected: all pass.

- [ ] **Step 6: Run full test suite**

```bash
cargo test 2>&1
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src/refs/java.rs
git commit -m "feat: implement JavaRefsAnalyzer Phase 2+3 (candidate collection + cross-reference)"
```

---

## Task 7: Wire `--refs` into `get.rs` + integration test

**Files:**
- Modify: `src/commands/get.rs`
- Test: `tests/integration_scan.rs`

- [ ] **Step 1: Write failing integration test for `--refs`**

Note: this test calls `compute_refs` directly (the public helper), not `run()`. The `run()` dispatcher path for `--refs` is verified by the manual smoke test in Step 6. This is an acceptable split — `compute_refs` tests the full analysis logic; `run()` just JSON-encodes and prints the result.

Add to `tests/integration_scan.rs`:

```rust
#[test]
fn test_refs_for_userservice() {
    use codeskel::cache::read_cache;

    let tmp = tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_refs_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache_path = tmp.path().join("cache.json");
    let cache = read_cache(&cache_path).unwrap();

    let svc_path = cache.files.keys()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in cache");

    let refs = codeskel::commands::get::compute_refs(&cache, &svc_path).unwrap();

    let user_path = cache.files.keys()
        .find(|p| p.ends_with("User.java"))
        .cloned()
        .expect("User.java must be in cache");
    let repo_path = cache.files.keys()
        .find(|p| p.contains("UserRepository"))
        .cloned()
        .expect("UserRepository.java must be in cache");

    let user_refs = refs.get(&user_path).expect("User.java must appear in refs");
    assert!(user_refs.contains(&"User".to_string()), "User type ref missing");
    assert!(user_refs.contains(&"getEmail".to_string()), "getEmail missing");

    let repo_refs = refs.get(&repo_path).expect("UserRepository.java must appear in refs");
    assert!(repo_refs.contains(&"UserRepository".to_string()), "UserRepository type ref missing");
    assert!(repo_refs.contains(&"findById".to_string()), "findById missing");
    assert!(repo_refs.contains(&"save".to_string()), "save missing");
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test test_refs_for_userservice -- --nocapture 2>&1
```

Expected: compile error — `compute_refs` does not exist.

- [ ] **Step 3: Implement `compute_refs` and `get_refs` in `get.rs`**

Add these imports to `src/commands/get.rs`:
```rust
use std::str::FromStr;
use crate::models::Language;
```

Add the two functions:

```rust
/// Builds the refs map for `file_path`: dep_file_path → [symbol names referenced].
/// Returns an error if the file is not in cache or its source cannot be read.
pub fn compute_refs(
    cache: &crate::models::CacheFile,
    file_path: &str,
) -> anyhow::Result<std::collections::HashMap<String, Vec<String>>> {
    let entry = cache.files.get(file_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found in cache", file_path))?;

    // Build import_map: simple_name → dep_file_path
    // simple_name = filename stem (strip .java / .py / etc.)
    let mut import_map = std::collections::HashMap::new();
    for dep_path in &entry.internal_imports {
        if let Some(stem) = std::path::Path::new(dep_path)
            .file_stem()
            .and_then(|s| s.to_str())
        {
            import_map.insert(stem.to_string(), dep_path.clone());
        }
    }

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

    // No internal imports → empty refs
    if import_map.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    // Read source from disk
    let source_path = std::path::Path::new(&cache.project_root).join(file_path);
    let source = std::fs::read_to_string(&source_path)
        .map_err(|e| anyhow::anyhow!("Cannot read '{}': {}", source_path.display(), e))?;

    // Dispatch to language-specific analyzer
    let lang = Language::from_str(&entry.language)
        .map_err(|e| anyhow::anyhow!("Unknown language '{}': {}", entry.language, e))?;

    match crate::refs::get_refs_analyzer(&lang) {
        Some(analyzer) => Ok(analyzer.extract_refs(&source, &import_map, &dep_sigs)),
        None => {
            eprintln!("[codeskel] --refs: language '{}' not yet supported; returning empty refs",
                entry.language);
            Ok(std::collections::HashMap::new())
        }
    }
}

fn get_refs(cache: &crate::models::CacheFile, file_path: &str) -> anyhow::Result<bool> {
    let refs = compute_refs(cache, file_path)?;
    let output = json!({ "for": file_path, "refs": refs });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(false)
}
```

- [ ] **Step 4: Run integration test**

```bash
cargo test test_refs_for_userservice -- --nocapture 2>&1
```

Expected: passes.

- [ ] **Step 5: Run full test suite**

```bash
cargo test 2>&1
```

Expected: all tests pass, zero failures.

- [ ] **Step 6: Smoke-test the CLI end-to-end**

```bash
cargo build 2>&1

# Scan the fixture
cargo run -- scan tests/fixtures/java_refs_project --cache-dir /tmp/cs_test

# Get chain count
cargo run -- get /tmp/cs_test/cache.json \
  --chain src/com/example/service/UserService.java

# Get chain entry 0 (should be User.java — the deepest dep)
cargo run -- get /tmp/cs_test/cache.json \
  --chain src/com/example/service/UserService.java --index 0

# Get refs
cargo run -- get /tmp/cs_test/cache.json \
  --refs src/com/example/service/UserService.java
```

Expected outputs:
- chain: `{ "for": "...", "count": 2 }`
- chain index 0: full FileEntry JSON for User.java
- refs: refs map with User.java and UserRepository.java as keys

- [ ] **Step 7: Final commit**

```bash
git add src/commands/get.rs tests/integration_scan.rs
git commit -m "feat: wire --refs into codeskel get (compute_refs + get_refs)"
```
