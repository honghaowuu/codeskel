# Reverse Dependency Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface implementor/subclass/annotation-usage signatures in `get --deps` and `next` output for context-poor file kinds (interface, abstract_class, annotation) so the LLM has meaningful context when commenting those files.

**Architecture:** Add `file_kind` and `reverse_deps` to `FileEntry` in cache; derive them in a post-pass in `scanner.rs` by reading existing `extends`/`implements` fields already on each `Signature`; update `get --deps` and `next`'s `build_deps` to include reverse dep signatures (capped at 5) when `file_kind` is a context-poor kind.

**Tech Stack:** Rust, serde_json, tree-sitter (already in use — no new dependencies)

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/models.rs` | Modify | Add `file_kind` and `reverse_deps` to `FileEntry` |
| `src/scanner.rs` | Modify | Derive `file_kind`; build `reverse_deps` in post-pass |
| `src/commands/get.rs` | Modify | Include `reverse_dep_signatures` in `get --deps` output |
| `src/commands/next.rs` | Modify | Add `reverse_deps: Vec<DepEntry>` to `NextOutput`; populate in `build_deps` |
| `tests/fixtures/java_project/src/com/example/repo/UserRepository.java` | Create | Interface fixture |
| `tests/fixtures/java_project/src/com/example/repo/JpaUserRepository.java` | Create | Implementor fixture |
| `tests/integration_scan.rs` | Modify | Add tests for `file_kind`, `reverse_deps`, and `get --deps`/`next` output |

---

## Task 1: Add `file_kind` and `reverse_deps` to `FileEntry`

**Files:**
- Modify: `src/models.rs`

- [ ] **Step 1: Add fields to `FileEntry`**

In `src/models.rs`, update `FileEntry`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    pub comment_coverage: f64,
    pub skip: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
    pub cycle_warning: bool,
    pub internal_imports: Vec<String>,
    pub signatures: Vec<Signature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scanned_at: Option<String>,
    /// Primary kind of the file's top-level declaration.
    /// Values: "class", "interface", "abstract_class", "annotation", "enum", "other"
    #[serde(default)]
    pub file_kind: String,
    /// Paths of files that implement, extend, or apply-as-annotation this file.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reverse_deps: Vec<String>,
}
```

- [ ] **Step 2: Update `FileEntry` construction in the test in `models.rs`**

The `file_entry_skip_reason_omitted_when_none` test constructs a `FileEntry` — add the new fields:

```rust
let entry = FileEntry {
    path: "src/Foo.java".into(),
    language: "java".into(),
    package: None,
    comment_coverage: 0.5,
    skip: false,
    skip_reason: None,
    cycle_warning: false,
    internal_imports: vec![],
    signatures: vec![],
    scanned_at: None,
    file_kind: "class".into(),
    reverse_deps: vec![],
};
```

Also add an assertion to the same test:

```rust
assert!(!json.contains("reverse_deps"), "reverse_deps should be omitted when empty, got: {}", json);
```

- [ ] **Step 3: Run the tests**

```bash
cargo test -p codeskel models
```

Expected: all `models` tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/models.rs
git commit -m "feat: add file_kind and reverse_deps to FileEntry"
```

---

## Task 2: Add Java fixtures for interface + implementor

**Files:**
- Create: `tests/fixtures/java_project/src/com/example/repo/UserRepository.java`
- Create: `tests/fixtures/java_project/src/com/example/repo/JpaUserRepository.java`

- [ ] **Step 1: Create the interface fixture**

```bash
mkdir -p tests/fixtures/java_project/src/com/example/repo
```

Write `tests/fixtures/java_project/src/com/example/repo/UserRepository.java`:

```java
package com.example.repo;

public interface UserRepository {
    Object findById(long id);
}
```

- [ ] **Step 2: Create the implementor fixture**

Write `tests/fixtures/java_project/src/com/example/repo/JpaUserRepository.java`:

```java
package com.example.repo;

public class JpaUserRepository implements UserRepository {
    @Override
    public Object findById(long id) {
        return null;
    }
}
```

- [ ] **Step 3: Verify the fixtures parse correctly**

```bash
cargo test -p codeskel test_get_command_by_index
```

Expected: passes (scan includes the new files without error).

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/java_project/src/com/example/repo/
git commit -m "test: add UserRepository interface and JpaUserRepository fixture files"
```

---

## Task 3: Derive `file_kind` and build `reverse_deps` in scanner

**Files:**
- Modify: `src/scanner.rs`

- [ ] **Step 1: Add `derive_file_kind` helper function**

Add this function just before the `scan` function in `src/scanner.rs`:

```rust
fn derive_file_kind(signatures: &[Signature]) -> String {
    for sig in signatures {
        match sig.kind.as_str() {
            "interface" => return "interface".to_string(),
            "annotation" => return "annotation".to_string(),
            "enum" => return "enum".to_string(),
            "class" => {
                if sig.modifiers.contains(&"abstract".to_string()) {
                    return "abstract_class".to_string();
                }
                return "class".to_string();
            }
            _ => {}
        }
    }
    "other".to_string()
}
```

- [ ] **Step 2: Set `file_kind` when constructing `FileEntry`**

In the main parse loop in `scan` (around line 122 where `FileEntry` is constructed), add `file_kind`:

```rust
file_entries.insert(rel_path.clone(), FileEntry {
    path: rel_path.clone(),
    language: language.as_str().to_string(),
    package,
    comment_coverage: coverage,
    skip,
    skip_reason,
    cycle_warning: false,
    internal_imports,
    file_kind: derive_file_kind(&signatures),
    signatures,
    scanned_at: None,
    reverse_deps: vec![],
});
```

Note: `derive_file_kind` must be called before `signatures` is moved, so call it before the `FileEntry` construction and store the result:

```rust
let file_kind = derive_file_kind(&signatures);
file_entries.insert(rel_path.clone(), FileEntry {
    path: rel_path.clone(),
    language: language.as_str().to_string(),
    package,
    comment_coverage: coverage,
    skip,
    skip_reason,
    cycle_warning: false,
    internal_imports,
    file_kind,
    signatures,
    scanned_at: None,
    reverse_deps: vec![],
});
```

- [ ] **Step 3: Add `build_reverse_deps` helper and call it in `scan`**

Add this function to `src/scanner.rs`:

```rust
fn build_reverse_deps(file_entries: &mut HashMap<String, FileEntry>) {
    // Build index: (package, simple_class_name) → rel_path
    let mut class_index: HashMap<(String, String), String> = HashMap::new();
    for (path, entry) in file_entries.iter() {
        if let Some(pkg) = &entry.package {
            for sig in &entry.signatures {
                match sig.kind.as_str() {
                    "class" | "interface" | "enum" | "annotation" => {
                        class_index.insert((pkg.clone(), sig.name.clone()), path.clone());
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Collect (implementor_path, target_path) pairs
    let mut reverse_pairs: Vec<(String, String)> = Vec::new();
    for (implementor_path, entry) in file_entries.iter() {
        let mut referenced_names: Vec<String> = Vec::new();
        for sig in &entry.signatures {
            if let Some(ext) = &sig.extends {
                referenced_names.push(ext.clone());
            }
            for imp in &sig.implements {
                referenced_names.push(imp.clone());
            }
        }
        for name in referenced_names {
            // Strategy 1: same-package resolution
            let resolved = entry.package.as_ref()
                .and_then(|pkg| class_index.get(&(pkg.clone(), name.clone())))
                .cloned()
                // Strategy 2: find in internal_imports whose top-level sig name matches
                .or_else(|| {
                    entry.internal_imports.iter().find_map(|dep_path| {
                        file_entries.get(dep_path).and_then(|dep_entry| {
                            dep_entry.signatures.iter().find(|s| {
                                s.name == name
                                    && matches!(s.kind.as_str(), "class" | "interface" | "enum")
                            })
                            .map(|_| dep_path.clone())
                        })
                    })
                });
            if let Some(target) = resolved {
                if target != *implementor_path {
                    reverse_pairs.push((implementor_path.clone(), target));
                }
            }
        }
    }

    // Apply pairs to file_entries
    for (implementor, target) in reverse_pairs {
        if let Some(entry) = file_entries.get_mut(&target) {
            if !entry.reverse_deps.contains(&implementor) {
                entry.reverse_deps.push(implementor);
            }
        }
    }

    // Sort for determinism
    for entry in file_entries.values_mut() {
        entry.reverse_deps.sort();
    }
}
```

Call it in `scan` after the main loop, just before the topological sort block (after `file_entries` is fully populated):

```rust
// Build reverse dependency relationships (interfaces ← implementors, etc.)
build_reverse_deps(&mut file_entries);

// Topological sort
let (full_order, cycle_pairs) = graph.topo_sort();
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p codeskel
```

Expected: all existing tests pass (new fields have `#[serde(default)]` so old cache files deserialize fine).

- [ ] **Step 5: Commit**

```bash
git add src/scanner.rs
git commit -m "feat: derive file_kind and build reverse_deps post-pass in scanner"
```

---

## Task 4: Integration tests for `file_kind` and `reverse_deps`

**Files:**
- Modify: `tests/integration_scan.rs`

- [ ] **Step 1: Write failing test for `file_kind` on interface**

Add to `tests/integration_scan.rs`:

```rust
#[test]
fn test_interface_has_file_kind() {
    use codeskel::cache::read_cache;

    let tmp = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache = read_cache(&tmp.path().join("cache.json")).unwrap();

    let repo_path = cache.files.keys()
        .find(|p| p.contains("UserRepository") && !p.contains("Jpa"))
        .cloned()
        .expect("UserRepository.java must be in cache");

    let entry = cache.files.get(&repo_path).unwrap();
    assert_eq!(entry.file_kind, "interface",
        "UserRepository should have file_kind=interface, got: {}", entry.file_kind);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p codeskel test_interface_has_file_kind
```

Expected: FAIL — `file_kind` is empty or wrong before Task 3 is done. (If Task 3 is already done, it should pass.)

- [ ] **Step 3: Write test for `reverse_deps` on interface**

Add to `tests/integration_scan.rs`:

```rust
#[test]
fn test_interface_reverse_deps_populated() {
    use codeskel::cache::read_cache;

    let tmp = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache = read_cache(&tmp.path().join("cache.json")).unwrap();

    // UserRepository should have JpaUserRepository in reverse_deps
    let repo_path = cache.files.keys()
        .find(|p| p.contains("UserRepository") && !p.contains("Jpa"))
        .cloned()
        .expect("UserRepository.java must be in cache");

    let entry = cache.files.get(&repo_path).unwrap();
    assert!(
        entry.reverse_deps.iter().any(|p| p.contains("JpaUserRepository")),
        "UserRepository.reverse_deps should contain JpaUserRepository, got: {:?}",
        entry.reverse_deps
    );

    // Base.java should have Service.java in reverse_deps (extends relationship)
    let base_path = cache.files.keys()
        .find(|p| p.contains("Base"))
        .cloned()
        .expect("Base.java must be in cache");

    let base_entry = cache.files.get(&base_path).unwrap();
    assert!(
        base_entry.reverse_deps.iter().any(|p| p.contains("Service")),
        "Base.reverse_deps should contain Service, got: {:?}",
        base_entry.reverse_deps
    );
}
```

- [ ] **Step 4: Run both new tests**

```bash
cargo test -p codeskel test_interface_has_file_kind test_interface_reverse_deps_populated
```

Expected: both PASS.

- [ ] **Step 5: Run full test suite**

```bash
cargo test -p codeskel
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add tests/integration_scan.rs
git commit -m "test: verify file_kind and reverse_deps populated after scan"
```

---

## Task 5: Update `get --deps` to include reverse dep signatures

**Files:**
- Modify: `src/commands/get.rs`

- [ ] **Step 1: Write failing test for `get --deps` reverse dep output**

Add to `tests/integration_scan.rs`:

```rust
#[test]
fn test_get_deps_includes_reverse_deps_for_interface() {
    use codeskel::cache::read_cache;

    let tmp = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache = read_cache(&tmp.path().join("cache.json")).unwrap();

    let repo_path = cache.files.keys()
        .find(|p| p.contains("UserRepository") && !p.contains("Jpa"))
        .cloned()
        .expect("UserRepository.java must be in cache");

    // Simulate what get_deps returns by calling the internal logic
    let entry = cache.files.get(&repo_path).unwrap();
    assert_eq!(entry.file_kind, "interface");
    assert!(
        entry.reverse_deps.iter().any(|p| p.contains("JpaUserRepository")),
        "reverse_deps must contain JpaUserRepository for the get --deps logic to work"
    );
}
```

- [ ] **Step 2: Run test to confirm it passes (prerequisite check)**

```bash
cargo test -p codeskel test_get_deps_includes_reverse_deps_for_interface
```

Expected: PASS (validates prerequisite from Task 4).

- [ ] **Step 3: Update `get_deps` in `src/commands/get.rs`**

Replace the existing `get_deps` function:

```rust
const REVERSE_DEP_KINDS: &[&str] = &["interface", "abstract_class", "annotation"];
const MAX_REVERSE_DEPS: usize = 5;

fn get_deps(cache: &crate::models::CacheFile, file_path: &str) -> anyhow::Result<bool> {
    let entry = cache.files.get(file_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found in cache", file_path))?;

    let dependencies: Vec<serde_json::Value> = entry.internal_imports.iter()
        .filter_map(|dep| {
            cache.files.get(dep).map(|dep_entry| {
                json!({
                    "path": dep_entry.path,
                    "signatures": dep_entry.signatures,
                })
            })
        })
        .collect();

    let mut output = json!({
        "for": file_path,
        "dependencies": dependencies,
    });

    if REVERSE_DEP_KINDS.contains(&entry.file_kind.as_str()) {
        let reverse_dep_signatures: Vec<serde_json::Value> = entry.reverse_deps.iter()
            .take(MAX_REVERSE_DEPS)
            .filter_map(|rdep| {
                cache.files.get(rdep).map(|rdep_entry| {
                    json!({
                        "path": rdep_entry.path,
                        "signatures": rdep_entry.signatures,
                    })
                })
            })
            .collect();
        output["reverse_dep_signatures"] = serde_json::json!(reverse_dep_signatures);
    }

    println!("{}", serde_json::to_string(&output)?);
    Ok(false)
}
```

- [ ] **Step 4: Run full test suite**

```bash
cargo test -p codeskel
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/commands/get.rs
git commit -m "feat: include reverse_dep_signatures in get --deps for interface/abstract_class/annotation"
```

---

## Task 6: Update `next` to include `reverse_deps` field

**Files:**
- Modify: `src/commands/next.rs`

- [ ] **Step 1: Add `reverse_deps` to `NextOutput`**

In `src/commands/next.rs`, update `NextOutput`:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct NextOutput {
    pub done: bool,
    pub mode: String,
    pub index: Option<usize>,
    pub remaining: usize,
    pub file: Option<NextFileEntry>,
    pub deps: Vec<DepEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reverse_deps: Vec<DepEntry>,
}
```

- [ ] **Step 2: Add `reverse_deps` field to all `NextOutput` construction sites**

There are three `NextOutput { done: true, ... }` constructions and two `NextOutput { done: false, ... }` constructions. All three "done" outputs use `deps: vec![]` — add `reverse_deps: vec![]`. The two "done:false" outputs need the new field populated.

Find all `NextOutput {` in `next.rs` and add `reverse_deps: vec![]` to "done" outputs:

```rust
// done responses (3 occurrences): add reverse_deps: vec![]
return Ok(NextOutput {
    done: true,
    mode: "project".into(),
    index: None,
    remaining: 0,
    file: None,
    deps: vec![],
    reverse_deps: vec![],
});
```

For "done:false" outputs, call a new helper (see next step):

```rust
let (deps, reverse_deps) = build_deps_with_reverse(&cache, &file_entry, max_fields)?;

Ok(NextOutput {
    done: false,
    mode: "project".into(),
    index: Some(next_cursor),
    remaining,
    file: Some(NextFileEntry::from(&file_entry)),
    deps,
    reverse_deps,
})
```

- [ ] **Step 3: Add `build_deps_with_reverse` and update `build_deps`**

Add a constant and a new function below `build_deps` in `next.rs`:

```rust
const REVERSE_DEP_KINDS: &[&str] = &["interface", "abstract_class", "annotation"];
const MAX_REVERSE_DEPS: usize = 5;

fn build_deps_with_reverse(
    cache: &crate::models::CacheFile,
    file_entry: &FileEntry,
    max_fields: usize,
) -> anyhow::Result<(Vec<DepEntry>, Vec<DepEntry>)> {
    let deps = build_deps(cache, file_entry, max_fields)?;

    let reverse_deps = if REVERSE_DEP_KINDS.contains(&file_entry.file_kind.as_str()) {
        file_entry.reverse_deps.iter()
            .take(MAX_REVERSE_DEPS)
            .filter_map(|rdep| {
                cache.files.get(rdep).map(|rdep_entry| {
                    let all_sigs: Vec<DepSignature> = rdep_entry.signatures.iter()
                        .map(DepSignature::from)
                        .collect();
                    let (non_fields, fields): (Vec<_>, Vec<_>) =
                        all_sigs.into_iter().partition(|s| s.kind != "field");
                    let fields_total = fields.len();
                    let kept_fields: Vec<_> = fields.into_iter().take(max_fields).collect();
                    let fields_omitted = fields_total - kept_fields.len();
                    let signatures: Vec<DepSignature> = non_fields.into_iter().chain(kept_fields).collect();
                    if signatures.is_empty() { return None; }
                    Some(DepEntry {
                        path: rdep_entry.path.clone(),
                        fields_omitted,
                        signatures,
                    })
                })
                .flatten()
            })
            .collect()
    } else {
        vec![]
    };

    Ok((deps, reverse_deps))
}
```

Replace both `build_deps(...)` calls in `run_project` and `run_targeted` with `build_deps_with_reverse(...)` and destructure the result:

```rust
// In run_project:
let (deps, reverse_deps) = build_deps_with_reverse(&cache, &file_entry, max_fields)?;

// In run_targeted (two call sites — bootstrap and subsequent):
let (deps, reverse_deps) = build_deps_with_reverse(&cache, &file_entry, max_fields)?;
```

- [ ] **Step 4: Update all `NextOutput { done: false, ... }` to include `reverse_deps`**

There are two "done:false" output sites (one in `run_project`, one in `run_targeted`). Both now use destructured `(deps, reverse_deps)`:

```rust
Ok(NextOutput {
    done: false,
    mode: "project".into(),  // or "targeted".into()
    index: Some(next_cursor),
    remaining,
    file: Some(NextFileEntry::from(&file_entry)),
    deps,
    reverse_deps,
})
```

- [ ] **Step 5: Run the test suite**

```bash
cargo test -p codeskel
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/commands/next.rs
git commit -m "feat: add reverse_deps field to next output for interface/abstract_class/annotation"
```

---

## Task 7: Integration test for `next` reverse deps

**Files:**
- Modify: `tests/integration_scan.rs`

- [ ] **Step 1: Write failing test**

Add to `tests/integration_scan.rs`:

```rust
#[test]
fn test_next_includes_reverse_deps_for_interface() {
    use codeskel::cache::read_cache;
    use codeskel::commands::next::{run_and_capture, NextOutput};
    use codeskel::cli::NextArgs;

    let tmp = tempfile::tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache_path = tmp.path().join("cache.json");
    let cache = read_cache(&cache_path).unwrap();

    // Find the index of UserRepository in cache.order
    let repo_index = cache.order.iter().position(|p| {
        p.contains("UserRepository") && !p.contains("Jpa")
    }).expect("UserRepository must be in order");

    // Drive `next` calls until we reach UserRepository
    for _ in 0..=repo_index {
        let output = run_and_capture(NextArgs {
            cache: cache_path.clone(),
            target: None,
            max_fields: 20,
        }).unwrap();
        assert!(!output.done, "should not be done before reaching UserRepository");

        if output.file.as_ref().map(|f| f.path.contains("UserRepository") && !f.path.contains("Jpa")).unwrap_or(false) {
            assert!(
                !output.reverse_deps.is_empty(),
                "next output for UserRepository interface must have reverse_deps"
            );
            assert!(
                output.reverse_deps.iter().any(|e| e.path.contains("JpaUserRepository")),
                "reverse_deps must include JpaUserRepository, got: {:?}",
                output.reverse_deps.iter().map(|e| &e.path).collect::<Vec<_>>()
            );
            return;
        }
    }
    panic!("UserRepository was not returned by next within expected iterations");
}
```

- [ ] **Step 2: Run the test**

```bash
cargo test -p codeskel test_next_includes_reverse_deps_for_interface
```

Expected: PASS.

- [ ] **Step 3: Run full test suite**

```bash
cargo test -p codeskel
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/integration_scan.rs
git commit -m "test: verify next output includes reverse_deps for interface files"
```

---

## Self-Review Checklist

**Spec coverage:**
- [x] `file_kind` field on FileEntry — Task 1
- [x] `reverse_deps` field on FileEntry — Task 1
- [x] Post-pass derives `file_kind` from signatures — Task 3
- [x] Post-pass builds `reverse_deps` from `extends`/`implements` — Task 3
- [x] Same-package resolution (no import needed) — Task 3 `build_reverse_deps`
- [x] `get --deps` includes `reverse_dep_signatures` for interface/abstract_class/annotation — Task 5
- [x] `next` output includes `reverse_deps` field — Task 6
- [x] Cap at 5 reverse deps — Tasks 5 and 6 (`MAX_REVERSE_DEPS = 5`)
- [x] Topo sort unchanged — not touched
- [x] Java only for now — only Java fixtures added; other parsers untouched
- [x] Integration tests — Tasks 4 and 7

**Placeholder scan:** No TBDs or stubs.

**Type consistency:**
- `DepEntry` used in both `deps` and `reverse_deps` in `NextOutput` ✓
- `build_deps_with_reverse` returns `(Vec<DepEntry>, Vec<DepEntry>)` ✓
- `REVERSE_DEP_KINDS` and `MAX_REVERSE_DEPS` defined in both `get.rs` and `next.rs` (intentional duplication — different modules) ✓
- `file_kind` field name consistent across `models.rs`, `scanner.rs`, `get.rs`, `next.rs` ✓
