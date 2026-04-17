# Output Size Control and Docstring Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `--max-fields N` flag to `next` to truncate field-heavy dep entries, and add `existing_word_count` to `Signature`/`DepSignature` to distinguish "no docstring" from "thin docstring".

**Architecture:** Two independent changes. Task 1 adds field truncation entirely within `src/commands/next.rs` and `src/cli.rs`. Task 2 adds a new field to `Signature` in `src/models.rs`, populates it in `src/scanner.rs`, and mirrors it onto `DepSignature` in `src/commands/next.rs`.

**Tech Stack:** Rust, serde (JSON serialization), clap (CLI args), cargo test (integration tests in `tests/integration_scan.rs`)

---

## File Map

| File | Change |
|------|--------|
| `src/cli.rs` | Add `max_fields: usize` to `NextArgs` (default 5) |
| `src/commands/next.rs` | Add `fields_omitted` to `DepEntry`; add `is_zero` helper; thread `max_fields` through `run_and_capture` → `run_project` → `run_targeted` → `build_deps`; add truncation logic; add `existing_word_count` to `DepSignature` + `From` impl |
| `src/models.rs` | Add `existing_word_count: usize` to `Signature`; add `is_zero` helper |
| `src/scanner.rs` | Restructure `apply_min_docstring_words` to unconditionally populate `existing_word_count` |
| `tests/integration_scan.rs` | Add tests for both changes |

---

## Task 1: `--max-fields` flag with kind-based field truncation

### Context

`build_deps` in `src/commands/next.rs` assembles dep entries for the `next` output. Currently it includes all signatures after refs filtering. For files with many fields (e.g. constants classes with 200 entries), this produces huge JSON output. We want to truncate field-kind signatures after N entries and report the overflow count.

`run_and_capture` calls either `run_project` or `run_targeted`, both of which call `build_deps`. The `max_fields` value must be passed through all of them.

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/commands/next.rs`
- Modify: `tests/integration_scan.rs`

---

- [ ] **Step 1: Write failing tests**

Add these two tests at the bottom of `tests/integration_scan.rs`, inside the existing test module (after the last `#[test]` function, before the closing `}`):

```rust
#[test]
fn test_next_max_fields_truncates_fields() {
    use codeskel::models::{CacheFile, FileEntry, Signature, Stats};
    use std::collections::HashMap;

    let tmp = tempdir().unwrap();

    // Build a dep file with 1 class + 8 fields
    let mut dep_sigs: Vec<Signature> = vec![Signature {
        kind: "class".into(), name: "AppExCode".into(),
        modifiers: vec![], params: None, return_type: None,
        throws: vec![], extends: None, implements: vec![], annotations: vec![],
        line: 1, has_docstring: false, docstring_text: None, existing_word_count: 0,
    }];
    for i in 0..8usize {
        dep_sigs.push(Signature {
            kind: "field".into(), name: format!("CODE_{}", i),
            modifiers: vec![], params: None, return_type: None,
            throws: vec![], extends: None, implements: vec![], annotations: vec![],
            line: i + 2, has_docstring: false, docstring_text: None, existing_word_count: 0,
        });
    }

    // Main file that imports the dep
    let main_sig = Signature {
        kind: "class".into(), name: "MyService".into(),
        modifiers: vec![], params: None, return_type: None,
        throws: vec![], extends: None, implements: vec![], annotations: vec![],
        line: 1, has_docstring: false, docstring_text: None, existing_word_count: 0,
    };

    let mut files = HashMap::new();
    files.insert("src/AppExCode.java".into(), FileEntry {
        path: "src/AppExCode.java".into(), language: "java".into(),
        package: None, comment_coverage: 0.0, skip: false, skip_reason: None,
        cycle_warning: false, internal_imports: vec![], signatures: dep_sigs, scanned_at: None,
    });
    files.insert("src/MyService.java".into(), FileEntry {
        path: "src/MyService.java".into(), language: "java".into(),
        package: None, comment_coverage: 0.0, skip: false, skip_reason: None,
        cycle_warning: false,
        internal_imports: vec!["src/AppExCode.java".into()],
        signatures: vec![main_sig], scanned_at: None,
    });

    let cache = CacheFile {
        version: 1,
        scanned_at: "2026-01-01T00:00:00Z".into(),
        project_root: tmp.path().to_string_lossy().into_owned(),
        detected_languages: vec!["java".into()],
        stats: Stats { total_files: 2, skipped_covered: 0, skipped_generated: 0, to_comment: 2 },
        min_docstring_words: 0,
        order: vec!["src/AppExCode.java".into(), "src/MyService.java".into()],
        files,
    };
    codeskel::cache::write_cache(tmp.path(), &cache).unwrap();

    // Advance past AppExCode to reach MyService (which has the dep)
    let args0 = codeskel::cli::NextArgs {
        cache: tmp.path().join("cache.json"),
        target: None,
        max_fields: 5,
    };
    codeskel::commands::next::run_and_capture(args0).unwrap(); // returns AppExCode

    let args1 = codeskel::cli::NextArgs {
        cache: tmp.path().join("cache.json"),
        target: None,
        max_fields: 5,
    };
    let out = codeskel::commands::next::run_and_capture(args1).unwrap();

    assert!(!out.done);
    assert_eq!(out.deps.len(), 1);
    let dep = &out.deps[0];
    // 1 class + 5 fields kept = 6 signatures; 3 fields omitted
    let field_count = dep.signatures.iter().filter(|s| s.kind == "field").count();
    assert_eq!(field_count, 5, "should keep exactly max_fields=5 fields");
    assert_eq!(dep.fields_omitted, 3, "should report 3 omitted fields");
}

#[test]
fn test_next_max_fields_zero_omits_all_fields() {
    use codeskel::models::{CacheFile, FileEntry, Signature, Stats};
    use std::collections::HashMap;

    let tmp = tempdir().unwrap();

    let mut dep_sigs: Vec<Signature> = vec![Signature {
        kind: "class".into(), name: "Constants".into(),
        modifiers: vec![], params: None, return_type: None,
        throws: vec![], extends: None, implements: vec![], annotations: vec![],
        line: 1, has_docstring: false, docstring_text: None, existing_word_count: 0,
    }];
    for i in 0..4usize {
        dep_sigs.push(Signature {
            kind: "field".into(), name: format!("CONST_{}", i),
            modifiers: vec![], params: None, return_type: None,
            throws: vec![], extends: None, implements: vec![], annotations: vec![],
            line: i + 2, has_docstring: false, docstring_text: None, existing_word_count: 0,
        });
    }

    let main_sig = Signature {
        kind: "class".into(), name: "Consumer".into(),
        modifiers: vec![], params: None, return_type: None,
        throws: vec![], extends: None, implements: vec![], annotations: vec![],
        line: 1, has_docstring: false, docstring_text: None, existing_word_count: 0,
    };

    let mut files = HashMap::new();
    files.insert("src/Constants.java".into(), FileEntry {
        path: "src/Constants.java".into(), language: "java".into(),
        package: None, comment_coverage: 0.0, skip: false, skip_reason: None,
        cycle_warning: false, internal_imports: vec![], signatures: dep_sigs, scanned_at: None,
    });
    files.insert("src/Consumer.java".into(), FileEntry {
        path: "src/Consumer.java".into(), language: "java".into(),
        package: None, comment_coverage: 0.0, skip: false, skip_reason: None,
        cycle_warning: false,
        internal_imports: vec!["src/Constants.java".into()],
        signatures: vec![main_sig], scanned_at: None,
    });

    let cache = CacheFile {
        version: 1,
        scanned_at: "2026-01-01T00:00:00Z".into(),
        project_root: tmp.path().to_string_lossy().into_owned(),
        detected_languages: vec!["java".into()],
        stats: Stats { total_files: 2, skipped_covered: 0, skipped_generated: 0, to_comment: 2 },
        min_docstring_words: 0,
        order: vec!["src/Constants.java".into(), "src/Consumer.java".into()],
        files,
    };
    codeskel::cache::write_cache(tmp.path(), &cache).unwrap();

    let args0 = codeskel::cli::NextArgs {
        cache: tmp.path().join("cache.json"), target: None, max_fields: 0,
    };
    codeskel::commands::next::run_and_capture(args0).unwrap();

    let args1 = codeskel::cli::NextArgs {
        cache: tmp.path().join("cache.json"), target: None, max_fields: 0,
    };
    let out = codeskel::commands::next::run_and_capture(args1).unwrap();

    let dep = &out.deps[0];
    let field_count = dep.signatures.iter().filter(|s| s.kind == "field").count();
    assert_eq!(field_count, 0, "max_fields=0 should omit all fields");
    assert_eq!(dep.fields_omitted, 4);
    // Non-field (class) still present
    assert!(dep.signatures.iter().any(|s| s.kind == "class"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test test_next_max_fields 2>&1 | tail -20
```

Expected: compile error — `max_fields` field not found on `NextArgs`, `existing_word_count` not found on `Signature`, `fields_omitted` not found on `DepEntry`.

- [ ] **Step 3: Add `max_fields` to `NextArgs` in `src/cli.rs`**

In `src/cli.rs`, find `NextArgs` and add the field:

```rust
#[derive(Args, Debug)]
pub struct NextArgs {
    /// Path to .codeskel/cache.json (default: .codeskel/cache.json)
    #[arg(long, default_value = ".codeskel/cache.json")]
    pub cache: std::path::PathBuf,

    /// Restrict loop to the transitive dep chain of this file (relative path)
    #[arg(long)]
    pub target: Option<String>,

    /// Maximum number of field-kind signatures to include per dep entry (0 = no fields)
    #[arg(long, default_value = "5")]
    pub max_fields: usize,
}
```

- [ ] **Step 4: Add `fields_omitted` to `DepEntry` and `is_zero` helper in `src/commands/next.rs`**

Add `is_zero` near the top of the file (after the `use` imports):

```rust
fn is_zero(n: &usize) -> bool { *n == 0 }
```

Update `DepEntry`:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct DepEntry {
    pub path: String,
    #[serde(skip_serializing_if = "is_zero", default)]
    pub fields_omitted: usize,
    pub signatures: Vec<DepSignature>,
}
```

- [ ] **Step 5: Thread `max_fields` through function signatures in `src/commands/next.rs`**

Update `run_and_capture`:

```rust
pub fn run_and_capture(args: NextArgs) -> anyhow::Result<NextOutput> {
    match args.target {
        Some(target) => run_targeted(args.cache, target, args.max_fields),
        None => run_project(args.cache, args.max_fields),
    }
}
```

Update `run_project` signature:
```rust
fn run_project(cache_path: std::path::PathBuf, max_fields: usize) -> anyhow::Result<NextOutput> {
```

Update every call to `build_deps` inside `run_project`:
```rust
let deps = build_deps(&cache, &file_entry, max_fields)?;
```

Update `run_targeted` signature:
```rust
fn run_targeted(cache_path: std::path::PathBuf, target: String, max_fields: usize) -> anyhow::Result<NextOutput> {
```

Update every call to `build_deps` inside `run_targeted`:
```rust
let deps = build_deps(&cache, &file_entry, max_fields)?;
```

Update `build_deps` signature:
```rust
fn build_deps(
    cache: &crate::models::CacheFile,
    file_entry: &FileEntry,
    max_fields: usize,
) -> anyhow::Result<Vec<DepEntry>> {
```

- [ ] **Step 6: Add field truncation logic inside `build_deps`**

Replace the `signatures` collection inside `build_deps` (the `.map(DepSignature::from).collect()` part) with:

```rust
let all_sigs: Vec<DepSignature> = dep_entry.signatures.iter()
    .filter(|sig| {
        if TOP_LEVEL_KINDS.contains(&sig.kind.as_str()) {
            return true;
        }
        match referenced {
            Some(names) if !names.is_empty() => names.contains(&sig.name),
            _ => true,
        }
    })
    .map(DepSignature::from)
    .collect();

// Truncate fields
let (non_fields, fields): (Vec<_>, Vec<_>) =
    all_sigs.into_iter().partition(|s| s.kind != "field");
let fields_total = fields.len();
let kept_fields: Vec<_> = fields.into_iter().take(max_fields).collect();
let fields_omitted = fields_total - kept_fields.len();

let signatures: Vec<DepSignature> = non_fields.into_iter().chain(kept_fields).collect();
if signatures.is_empty() {
    return None;
}
Some(DepEntry {
    path: dep_entry.path.clone(),
    fields_omitted,
    signatures,
})
```

The full updated `build_deps` should look like this (replacing the existing `entries` binding):

```rust
let entries = file_entry.internal_imports.iter()
    .filter_map(|dep| {
        cache.files.get(dep).map(|dep_entry| {
            let referenced = refs_map.get(dep.as_str());
            let all_sigs: Vec<DepSignature> = dep_entry.signatures.iter()
                .filter(|sig| {
                    if TOP_LEVEL_KINDS.contains(&sig.kind.as_str()) {
                        return true;
                    }
                    match referenced {
                        Some(names) if !names.is_empty() => names.contains(&sig.name),
                        _ => true,
                    }
                })
                .map(DepSignature::from)
                .collect();

            let (non_fields, fields): (Vec<_>, Vec<_>) =
                all_sigs.into_iter().partition(|s| s.kind != "field");
            let fields_total = fields.len();
            let kept_fields: Vec<_> = fields.into_iter().take(max_fields).collect();
            let fields_omitted = fields_total - kept_fields.len();

            let signatures: Vec<DepSignature> = non_fields.into_iter().chain(kept_fields).collect();
            if signatures.is_empty() {
                return None;
            }
            Some(DepEntry {
                path: dep_entry.path.clone(),
                fields_omitted,
                signatures,
            })
        })
        .flatten()
    })
    .collect();
```

- [ ] **Step 7: Fix all other `NextArgs` construction sites** (the tests build `NextArgs` directly — add `max_fields: 0` or `max_fields: 5` to all existing `NextArgs { ... }` literals in `tests/integration_scan.rs` that don't already have it)

Search for all occurrences:
```bash
grep -n "NextArgs {" tests/integration_scan.rs
```

For each occurrence, add `max_fields: 0,` (using 0 in existing tests preserves current behavior — no truncation). Example before/after:

```rust
// Before:
let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None };
// After:
let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
```

- [ ] **Step 8: Run tests**

```bash
cargo test 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/cli.rs src/commands/next.rs tests/integration_scan.rs
git commit -m "feat: add --max-fields flag to next with kind-based field truncation"
```

---

## Task 2: `existing_word_count` on `Signature` and `DepSignature`

### Context

`Signature` in `src/models.rs` has `has_docstring: bool` and `docstring_text: Option<String>`. When `--min-docstring-words N` is used during scan, `has_docstring` can be `false` even when a docstring exists but is too short. Agents need to distinguish "no doc" from "thin doc" without reading source files.

`apply_min_docstring_words` in `src/scanner.rs` currently has an early-exit (`if min_words > 0`) that skips the loop entirely when the threshold is 0. We need to restructure it so `existing_word_count` is populated unconditionally, while the `has_docstring` downgrade remains gated on `min_words > 0`.

`DepSignature` in `src/commands/next.rs` mirrors a subset of `Signature` fields for dep context output. It needs `existing_word_count` too, with its `From<&Signature>` impl updated.

**Files:**
- Modify: `src/models.rs`
- Modify: `src/scanner.rs`
- Modify: `src/commands/next.rs`
- Modify: `tests/integration_scan.rs`

---

- [ ] **Step 1: Write failing tests**

Add these two tests to `tests/integration_scan.rs`:

```rust
#[test]
fn test_existing_word_count_thin_docstring() {
    // A file with a short docstring (< min_docstring_words) should have
    // has_docstring=false but existing_word_count > 0.
    use std::io::Write;
    let tmp = tempdir().unwrap();
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let mut f = std::fs::File::create(src_dir.join("Thin.java")).unwrap();
    writeln!(f, "/** Short doc. */").unwrap();
    writeln!(f, "public class Thin {{}}").unwrap();

    codeskel::scanner::scan(tmp.path(), &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 30, // threshold higher than "Short doc."
        cache_dir: Some(tmp.path().join(".codeskel")),
        verbose: false,
    }).unwrap();

    let cache = codeskel::cache::read_cache(&tmp.path().join(".codeskel/cache.json")).unwrap();
    let entry = cache.files.get("src/Thin.java").unwrap();
    let cls = entry.signatures.iter().find(|s| s.kind == "class").unwrap();

    assert!(!cls.has_docstring, "thin doc should fail min_docstring_words threshold");
    assert!(cls.existing_word_count > 0, "existing_word_count should reflect actual word count");
}

#[test]
fn test_existing_word_count_populated_with_min_words_zero() {
    // Even when min_docstring_words=0 (presence-only mode), existing_word_count
    // should still be populated for signatures that have a docstring.
    use std::io::Write;
    let tmp = tempdir().unwrap();
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let mut f = std::fs::File::create(src_dir.join("Documented.java")).unwrap();
    writeln!(f, "/** This is a well documented class with many words here. */").unwrap();
    writeln!(f, "public class Documented {{}}").unwrap();

    codeskel::scanner::scan(tmp.path(), &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0, // presence-only
        cache_dir: Some(tmp.path().join(".codeskel")),
        verbose: false,
    }).unwrap();

    let cache = codeskel::cache::read_cache(&tmp.path().join(".codeskel/cache.json")).unwrap();
    let entry = cache.files.get("src/Documented.java").unwrap();
    let cls = entry.signatures.iter().find(|s| s.kind == "class").unwrap();

    assert!(cls.has_docstring, "with min_docstring_words=0, any doc counts");
    assert!(cls.existing_word_count > 0, "existing_word_count should be populated even when min_words=0");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test test_existing_word_count 2>&1 | tail -20
```

Expected: compile error — `existing_word_count` field not found on `Signature`.

- [ ] **Step 3: Add `existing_word_count` to `Signature` in `src/models.rs`**

Add `is_zero` helper near the top of the file (after the `use` imports, before any struct definitions):

```rust
fn is_zero(n: &usize) -> bool { *n == 0 }
```

Add the field to `Signature` after `has_docstring`:

```rust
pub has_docstring: bool,
#[serde(skip_serializing_if = "is_zero", default)]
pub existing_word_count: usize,
#[serde(skip_serializing_if = "Option::is_none")]
pub docstring_text: Option<String>,
```

- [ ] **Step 4: Fix `Signature` construction sites in `src/models.rs` tests**

In the `#[cfg(test)]` block at the bottom of `src/models.rs`, find the `Signature { ... }` literal in `test_signature_roundtrip` and add `existing_word_count: 0`:

```rust
let sig = Signature {
    kind: "method".into(),
    name: "findByEmail".into(),
    // ... other fields ...
    has_docstring: false,
    existing_word_count: 0,
    docstring_text: None,
};
```

- [ ] **Step 5: Fix all `Signature { ... }` construction sites across the codebase**

Search for all struct literal constructions (parsers, tests, everywhere):
```bash
grep -rn "Signature {" src/ tests/integration_scan.rs
```

This will surface ~30+ sites across `src/parsers/java.rs`, `python.rs`, `typescript.rs`, `javascript.rs`, `go.rs`, `rust_lang.rs`, `csharp.rs`, `cpp.rs`, `ruby.rs`, `src/scanner.rs`, and `tests/integration_scan.rs`. Add `existing_word_count: 0,` to every one that doesn't already have it. The parser files set `existing_word_count: 0` at parse time; `apply_min_docstring_words` will overwrite it with the real count during scan.

- [ ] **Step 6: Restructure `apply_min_docstring_words` in `src/scanner.rs`**

Replace the current function body with:

```rust
pub fn apply_min_docstring_words(signatures: &mut Vec<Signature>, min_words: usize) -> f64 {
    for sig in signatures.iter_mut() {
        let words = sig.docstring_text.as_deref()
            .map(count_prose_words)
            .unwrap_or(0);
        sig.existing_word_count = words;
        if min_words > 0 && sig.has_docstring && words < min_words {
            sig.has_docstring = false;
        }
    }

    let documentable: Vec<&Signature> = signatures.iter()
        .filter(|s| matches!(s.kind.as_str(),
            "class" | "interface" | "enum" | "method" | "constructor" | "field" |
            "function" | "struct" | "trait" | "type" | "type_alias"))
        .collect();
    let documented = documentable.iter().filter(|s| s.has_docstring).count();
    let total = documentable.len();
    if total > 0 { documented as f64 / total as f64 } else { 1.0 }
}
```

Key changes from the original:
- Loop runs unconditionally (no outer `if min_words > 0`)
- `existing_word_count` is set for every signature
- `has_docstring` downgrade is conditional on `min_words > 0 && words < min_words`

- [ ] **Step 7: Add `existing_word_count` to `DepSignature` in `src/commands/next.rs`**

Update the `DepSignature` struct (add after `docstring_text`):

```rust
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
    #[serde(skip_serializing_if = "is_zero", default)]
    pub existing_word_count: usize,
}
```

Update the `From<&Signature>` impl to include the new field:

```rust
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
            existing_word_count: sig.existing_word_count,
        }
    }
}
```

- [ ] **Step 8: Run all tests**

```bash
cargo test 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 9: Verify JSON output omits `existing_word_count` when zero**

```bash
cargo build 2>&1 | tail -5
echo '{}' | cargo run -- next --cache .codeskel/cache.json 2>/dev/null || true
```

If you have a real project scanned, check the JSON output:
```bash
cargo run -- next --cache /path/to/.codeskel/cache.json | python3 -c "
import json,sys
data=json.load(sys.stdin)
for dep in data.get('deps',[]):
    for sig in dep['signatures']:
        assert 'existing_word_count' not in sig or sig['existing_word_count'] > 0, \
            f'zero word count should be omitted: {sig}'
print('OK: zero word counts correctly omitted')
"
```

- [ ] **Step 10: Commit**

```bash
git add src/models.rs src/scanner.rs src/commands/next.rs tests/integration_scan.rs
git commit -m "feat: add existing_word_count to Signature and DepSignature"
```
