# Compact Output Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make all command output compact JSON, and make `next` deps carry only the signals Claude actually needs to document the current file.

**Architecture:** Four independent changes applied in order: (1) swap serializer everywhere, (2) replace `FileEntry` in `next` output with a slimmer `NextFileEntry`, (3) replace `Signature` in deps with a slimmer `DepSignature`, (4) wire `compute_refs` into `build_deps` to filter dep signatures to only the symbols the current file references.

**Tech Stack:** Rust, serde/serde_json, existing `compute_refs` in `src/commands/get.rs`, existing `java_refs_project` test fixture.

---

## File Map

| File | What changes |
|---|---|
| `src/commands/scan.rs` | `to_string_pretty` → `to_string` |
| `src/commands/get.rs` | `to_string_pretty` → `to_string` (5 call sites) |
| `src/commands/pom.rs` | `to_string_pretty` → `to_string` (2 call sites) |
| `src/commands/next.rs` | Add `NextFileEntry`, `DepSignature`; update `NextOutput`, `DepEntry`; update `build_deps` to be fallible + refs-filtered |
| `tests/integration_scan.rs` | Add 3 new tests (compact JSON, no-skip/internal_imports, refs-filtered deps) |

---

### Task 1: Compact JSON for all commands

**Files:**
- Modify: `src/commands/scan.rs:44`
- Modify: `src/commands/get.rs:62,107,121,144,210`
- Modify: `src/commands/pom.rs:255,260`
- Modify: `src/commands/next.rs:29`

- [ ] **Step 1: Write the failing test**

Add to `tests/integration_scan.rs`:

```rust
#[test]
fn next_output_is_compact_json() {
    use std::process::Command;
    let tmp = tempdir().unwrap();
    make_cache_in("java_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let output = Command::new(env!("CARGO_BIN_EXE_codeskel"))
        .args(["next", "--cache", cache_path.to_str().unwrap()])
        .output()
        .expect("failed to run codeskel");

    let stdout = String::from_utf8(output.stdout).unwrap();
    // Compact JSON has no newlines (except possibly a trailing one)
    let trimmed = stdout.trim();
    assert!(!trimmed.contains('\n'), "output should be single-line compact JSON, got:\n{}", trimmed);
    // Should still be valid JSON
    let _: serde_json::Value = serde_json::from_str(trimmed).expect("output must be valid JSON");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test next_output_is_compact_json -- --nocapture 2>&1 | tail -20
```

Expected: FAIL — output contains newlines (pretty-printed).

- [ ] **Step 3: Replace all `to_string_pretty` with `to_string`**

In `src/commands/scan.rs` line 44:
```rust
println!("{}", serde_json::to_string(&summary)?);
```

In `src/commands/get.rs` — all 5 occurrences (lines 62, 107, 121, 144, 210):
```rust
println!("{}", serde_json::to_string(entry)?);
// and
println!("{}", serde_json::to_string(&output)?);
```

In `src/commands/pom.rs` lines 255 and 260:
```rust
let json = serde_json::to_string(&output)?;
```

In `src/commands/next.rs` line 29:
```rust
println!("{}", serde_json::to_string(&output)?);
```

- [ ] **Step 4: Run all tests**

```bash
cargo test 2>&1 | tail -20
```

Expected: all pass including the new compact JSON test.

- [ ] **Step 5: Commit**

```bash
git add src/commands/scan.rs src/commands/get.rs src/commands/pom.rs src/commands/next.rs tests/integration_scan.rs
git commit -m "feat: switch all command output to compact JSON"
```

---

### Task 2: NextFileEntry — strip redundant fields from `next` file output

**Files:**
- Modify: `src/commands/next.rs` — add `NextFileEntry` struct and update `NextOutput`
- Modify: `tests/integration_scan.rs` — add test

- [ ] **Step 1: Write the failing test**

Add to `tests/integration_scan.rs`:

```rust
#[test]
fn next_file_entry_omits_skip_and_internal_imports() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    // Advance twice to get a file that has internal_imports (UserService depends on User + UserRepository)
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None };
    codeskel::commands::next::run_and_capture(args).unwrap(); // index 0
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None };
    codeskel::commands::next::run_and_capture(args).unwrap(); // index 1
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None };
    let output = codeskel::commands::next::run_and_capture(args).unwrap(); // index 2: UserService

    assert!(!output.done);
    let json = serde_json::to_string(&output.file).unwrap();
    assert!(!json.contains("\"skip\""),
        "skip should not appear in next file output, got: {}", json);
    assert!(!json.contains("\"internal_imports\""),
        "internal_imports should not appear in next file output, got: {}", json);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test next_file_entry_omits_skip_and_internal_imports -- --nocapture 2>&1 | tail -20
```

Expected: FAIL — JSON contains `"skip"` and `"internal_imports"`.

- [ ] **Step 3: Add `NextFileEntry` to `src/commands/next.rs`**

Add after the existing imports, before `DepEntry`:

```rust
use crate::models::{FileEntry, Param, Signature};

/// Slimmed-down file entry for `next` output — omits fields that are always
/// false/empty in the loop or redundant with `deps`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextFileEntry {
    pub path: String,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    pub comment_coverage: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cycle_warning: bool,
    pub signatures: Vec<Signature>,
}

impl From<&FileEntry> for NextFileEntry {
    fn from(fe: &FileEntry) -> Self {
        NextFileEntry {
            path: fe.path.clone(),
            language: fe.language.clone(),
            package: fe.package.clone(),
            comment_coverage: fe.comment_coverage,
            cycle_warning: fe.cycle_warning,
            signatures: fe.signatures.clone(),
        }
    }
}
```

- [ ] **Step 4: Update `NextOutput.file` type**

In `src/commands/next.rs`, change `NextOutput`:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct NextOutput {
    pub done: bool,
    pub mode: String,
    pub index: Option<usize>,
    pub remaining: usize,
    pub file: Option<NextFileEntry>,   // was Option<FileEntry>
    pub deps: Vec<DepEntry>,
}
```

- [ ] **Step 5: Update all `file: Some(file_entry)` sites in `next.rs`**

There are three `file: Some(file_entry)` sites — one in `run_project` and two in `run_targeted`. Change each to:

```rust
file: Some(NextFileEntry::from(&file_entry)),
```

Also remove the now-unused `FileEntry` from the use statement if it's only used via `NextFileEntry::from`.

- [ ] **Step 6: Run all tests**

```bash
cargo test 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src/commands/next.rs tests/integration_scan.rs
git commit -m "feat: use NextFileEntry in next output — drop skip and internal_imports"
```

---

### Task 3: DepSignature — strip `has_docstring` and `line` from dep signatures

**Files:**
- Modify: `src/commands/next.rs` — add `DepSignature`, update `DepEntry`, update `build_deps`
- Modify: `tests/integration_scan.rs` — add test

- [ ] **Step 1: Write the failing test**

Add to `tests/integration_scan.rs`:

```rust
#[test]
fn next_dep_signatures_omit_has_docstring_and_line() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    // Advance to a file that has deps
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None };
    codeskel::commands::next::run_and_capture(args).unwrap(); // index 0
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None };
    codeskel::commands::next::run_and_capture(args).unwrap(); // index 1
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None };
    let output = codeskel::commands::next::run_and_capture(args).unwrap(); // index 2: UserService

    assert!(!output.deps.is_empty(), "UserService must have deps");
    let deps_json = serde_json::to_string(&output.deps).unwrap();
    assert!(!deps_json.contains("\"has_docstring\""),
        "has_docstring must not appear in dep signatures, got: {}", deps_json);
    assert!(!deps_json.contains("\"line\""),
        "line must not appear in dep signatures, got: {}", deps_json);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test next_dep_signatures_omit_has_docstring_and_line -- --nocapture 2>&1 | tail -20
```

Expected: FAIL — deps JSON contains `"has_docstring"` and `"line"`.

- [ ] **Step 3: Add `DepSignature` to `src/commands/next.rs`**

Replace the existing `DepEntry` definition with:

```rust
/// Signature stripped for dep context — no `has_docstring` or `line`,
/// since Claude uses dep signatures for understanding, not for documenting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepSignature {
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub modifiers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<Param>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub throws: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub implements: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub annotations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring_text: Option<String>,
}

impl From<&Signature> for DepSignature {
    fn from(sig: &Signature) -> Self {
        DepSignature {
            kind: sig.kind.clone(),
            name: sig.name.clone(),
            modifiers: sig.modifiers.clone(),
            params: sig.params.clone(),
            return_type: sig.return_type.clone(),
            throws: sig.throws.clone(),
            extends: sig.extends.clone(),
            implements: sig.implements.clone(),
            annotations: sig.annotations.clone(),
            docstring_text: sig.docstring_text.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DepEntry {
    pub path: String,
    pub signatures: Vec<DepSignature>,   // was Vec<Signature>
}
```

- [ ] **Step 4: Update `build_deps` to convert `Signature` → `DepSignature`**

Replace the existing `build_deps` function:

```rust
fn build_deps(cache: &crate::models::CacheFile, file_entry: &FileEntry) -> Vec<DepEntry> {
    file_entry.internal_imports.iter()
        .filter_map(|dep| {
            cache.files.get(dep).map(|dep_entry| DepEntry {
                path: dep_entry.path.clone(),
                signatures: dep_entry.signatures.iter()
                    .map(DepSignature::from)
                    .collect(),
            })
        })
        .collect()
}
```

(No refs filtering yet — that comes in Task 4.)

- [ ] **Step 5: Run all tests**

```bash
cargo test 2>&1 | tail -20
```

Expected: all pass including the new DepSignature test.

- [ ] **Step 6: Commit**

```bash
git add src/commands/next.rs tests/integration_scan.rs
git commit -m "feat: use DepSignature in next deps — drop has_docstring and line"
```

---

### Task 4: Wire refs into `build_deps` — filter dep signatures to referenced symbols

**Files:**
- Modify: `src/commands/next.rs` — make `build_deps` fallible, add refs call, add filtering logic
- Modify: `tests/integration_scan.rs` — add refs-filtered test

- [ ] **Step 1: Write the failing test**

`UserService.java` uses `getEmail` from `User` but not `setEmail`. After refs filtering, `User`'s dep signatures should include `User` (top-level type) and `getEmail` but NOT `setEmail`.

Add to `tests/integration_scan.rs`:

```rust
#[test]
fn next_deps_filtered_to_referenced_symbols() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    // Advance to UserService (index 2 in topo order: User → UserRepository → UserService)
    for _ in 0..2 {
        let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None };
        codeskel::commands::next::run_and_capture(args).unwrap();
    }
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None };
    let output = codeskel::commands::next::run_and_capture(args).unwrap();

    assert!(!output.done);
    let file_path = output.file.as_ref().unwrap().path.as_str();
    assert!(file_path.contains("UserService"), "expected UserService, got: {}", file_path);

    // Find the User dep
    let user_dep = output.deps.iter()
        .find(|d| d.path.contains("User.java") && !d.path.contains("UserRepository"))
        .expect("User dep must be present");

    let sig_names: Vec<&str> = user_dep.signatures.iter()
        .map(|s| s.name.as_str())
        .collect();

    // User class (top-level type) must always be present
    assert!(sig_names.contains(&"User"), "User class must be in dep signatures, got: {:?}", sig_names);
    // getEmail is referenced in UserService → must be included
    assert!(sig_names.contains(&"getEmail"), "getEmail must be included (referenced), got: {:?}", sig_names);
    // setEmail is NOT referenced in UserService → must be filtered out
    assert!(!sig_names.contains(&"setEmail"), "setEmail must be filtered out (unreferenced), got: {:?}", sig_names);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test next_deps_filtered_to_referenced_symbols -- --nocapture 2>&1 | tail -20
```

Expected: FAIL — `setEmail` is still present in dep signatures.

- [ ] **Step 3: Make `build_deps` fallible and add refs-based filtering**

Replace `build_deps` in `src/commands/next.rs`:

```rust
const TOP_LEVEL_KINDS: &[&str] = &["class", "interface", "enum", "struct", "trait", "type_alias"];

fn build_deps(
    cache: &crate::models::CacheFile,
    file_entry: &FileEntry,
) -> anyhow::Result<Vec<DepEntry>> {
    // Attempt refs analysis; on failure, fall back to unfiltered (all deps get all sigs)
    let refs_map = match crate::commands::get::compute_refs(cache, &file_entry.path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "[codeskel] Warning: refs analysis failed for '{}': {}; using unfiltered deps",
                file_entry.path, e
            );
            std::collections::HashMap::new()
        }
    };

    let entries = file_entry.internal_imports.iter()
        .filter_map(|dep| {
            cache.files.get(dep).map(|dep_entry| {
                let referenced = refs_map.get(dep.as_str());
                let signatures: Vec<DepSignature> = dep_entry.signatures.iter()
                    .filter(|sig| {
                        // Top-level type declarations are always included as structural anchors
                        if TOP_LEVEL_KINDS.contains(&sig.kind.as_str()) {
                            return true;
                        }
                        // Non-empty refs list → filter to referenced names only
                        // Absent key or empty list → fallback: include all
                        match referenced {
                            Some(names) if !names.is_empty() => names.contains(&sig.name),
                            _ => true,
                        }
                    })
                    .map(DepSignature::from)
                    .collect();
                DepEntry {
                    path: dep_entry.path.clone(),
                    signatures,
                }
            })
        })
        .collect();

    Ok(entries)
}
```

- [ ] **Step 4: Update `build_deps` call sites in `run_project` and `run_targeted`**

Both sites currently do `let deps = build_deps(&cache, &file_entry);`. Change each to:

```rust
let deps = build_deps(&cache, &file_entry)?;
```

- [ ] **Step 5: Run all tests**

```bash
cargo test 2>&1 | tail -20
```

Expected: all pass including `next_deps_filtered_to_referenced_symbols`.

- [ ] **Step 6: Commit**

```bash
git add src/commands/next.rs tests/integration_scan.rs
git commit -m "feat: filter next deps to referenced symbols via refs analysis"
```
