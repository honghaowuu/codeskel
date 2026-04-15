# `codeskel next` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `codeskel next` — a single command that fuses rescan + advance + fetch into one atomic call, making rescan structurally unavoidable in the commenting loop.

**Architecture:** A `session.json` file alongside `cache.json` tracks the current loop cursor. On each call, `next` rescans the previously returned file (if any), advances the cursor, and returns the next file + its dep signatures in one JSON response. A shared `rescan_one()` helper is extracted from `commands/rescan.rs` so both `rescan` and `next` share the same parse/update logic.

**Tech Stack:** Rust, clap, serde_json, anyhow, chrono (already in use)

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/session.rs` | **Create** | `Session` struct + `read_session`, `write_session`, `delete_session` |
| `src/commands/next.rs` | **Create** | `next` command: bootstrap / advance / done logic |
| `src/commands/rescan.rs` | **Modify** | Extract `pub fn rescan_one` and `pub fn recompute_stats` |
| `src/cli.rs` | **Modify** | Add `NextArgs`, `Commands::Next` variant |
| `src/commands/mod.rs` | **Modify** | `pub mod next` |
| `src/lib.rs` | **Modify** | `pub mod session` |
| `src/main.rs` | **Modify** | `Commands::Next(args) => commands::next::run(args)` |
| `src/commands/scan.rs` | **Modify** | Call `delete_session` on successful scan |
| `tests/integration_scan.rs` | **Modify** | Integration tests for `next` lifecycle |

---

## Task 1: Add `src/session.rs`

**Files:**
- Create: `src/session.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing tests in `src/session.rs`**

```rust
// src/session.rs
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Session {
    pub cursor: i64,  // -1 = not started / complete
    pub current_file: Option<String>,
}

pub fn read_session(cache_dir: &Path) -> Session {
    let path = cache_dir.join("session.json");
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Session { cursor: -1, current_file: None },
    }
}

pub fn write_session(cache_dir: &Path, session: &Session) -> anyhow::Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    let path = cache_dir.join("session.json");
    let json = serde_json::to_string_pretty(session)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn delete_session(cache_dir: &Path) {
    let path = cache_dir.join("session.json");
    let _ = std::fs::remove_file(path); // silently ignore if missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn read_missing_returns_default() {
        let dir = tempdir().unwrap();
        let session = read_session(dir.path());
        assert_eq!(session.cursor, -1);
        assert_eq!(session.current_file, None);
    }

    #[test]
    fn write_read_roundtrip() {
        let dir = tempdir().unwrap();
        let s = Session { cursor: 3, current_file: Some("src/Foo.java".into()) };
        write_session(dir.path(), &s).unwrap();
        let back = read_session(dir.path());
        assert_eq!(back, s);
    }

    #[test]
    fn delete_removes_file() {
        let dir = tempdir().unwrap();
        let s = Session { cursor: 0, current_file: Some("x".into()) };
        write_session(dir.path(), &s).unwrap();
        assert!(dir.path().join("session.json").exists());
        delete_session(dir.path());
        assert!(!dir.path().join("session.json").exists());
    }

    #[test]
    fn delete_missing_is_silent() {
        let dir = tempdir().unwrap();
        delete_session(dir.path()); // must not panic
    }
}
```

- [ ] **Step 2: Register module in `src/lib.rs`**

Add `pub mod session;` after `pub mod scanner;`.

- [ ] **Step 3: Run tests to verify they pass**

```bash
cargo test session -- --nocapture
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/session.rs src/lib.rs
git commit -m "feat: add session.rs (read/write/delete session state for next command)"
```

---

## Task 2: Extract `rescan_one` and `recompute_stats` from `commands/rescan.rs`

**Files:**
- Modify: `src/commands/rescan.rs`

The existing `run()` function contains inline rescan logic and stats recomputation. Extract these into two `pub fn` so `commands/next.rs` can reuse them without duplicating code.

- [ ] **Step 1: Extract the functions**

Replace the body of `rescan.rs` with:

```rust
use crate::cache::{read_cache, write_cache};
use crate::cli::RescanArgs;
use crate::generated::is_generated;
use crate::lang::detect_language;
use crate::models::CacheFile;
use crate::parsers::get_parser;
use crate::scanner::apply_min_docstring_words;
use chrono::Utc;
use std::path::Path;

/// Re-parse a single file and update its cache entry.
/// Returns `true` if a warning was emitted (file unreadable / language unknown).
/// Does NOT recompute stats or write cache — callers must do that.
pub fn rescan_one(cache: &mut CacheFile, rel: &str) -> bool {
    let root = Path::new(&cache.project_root);
    let abs = root.join(rel);

    let content = match std::fs::read_to_string(&abs) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[codeskel] Warning: cannot read {}: {}", rel, e);
            return true;
        }
    };

    let lang = match detect_language(&abs) {
        Some(l) => l,
        None => {
            eprintln!("[codeskel] Warning: cannot detect language for {}", rel);
            return true;
        }
    };

    let generated = is_generated(rel, &content);
    let pr = get_parser(&lang).parse(&content);

    if let Some(entry) = cache.files.get_mut(rel) {
        let mut sigs = pr.signatures;
        let cov = apply_min_docstring_words(&mut sigs, cache.min_docstring_words);
        entry.comment_coverage = cov;
        entry.signatures = sigs;
        entry.scanned_at = Some(Utc::now().to_rfc3339());
        if generated {
            entry.skip = true;
            entry.skip_reason = Some("generated".to_string());
        }
    } else {
        eprintln!("[codeskel] Warning: {} not found in cache, skipping", rel);
        return true;
    }

    false
}

/// Recompute `cache.stats` from current `cache.files` and `cache.order`.
pub fn recompute_stats(cache: &mut CacheFile) {
    let total = cache.files.len();
    let skipped_covered = cache.files.values()
        .filter(|e| e.skip_reason.as_deref() == Some("sufficient_coverage"))
        .count();
    let skipped_generated = cache.files.values()
        .filter(|e| e.skip_reason.as_deref() == Some("generated"))
        .count();
    let to_comment = cache.order.iter()
        .filter(|p| cache.files.get(*p).map(|e| !e.skip).unwrap_or(false))
        .count();

    cache.stats = crate::models::Stats {
        total_files: total,
        skipped_covered,
        skipped_generated,
        to_comment,
    };
}

pub fn run(args: RescanArgs) -> anyhow::Result<bool> {
    let mut cache = read_cache(&args.cache_path)?;
    let cache_dir = args.cache_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut warnings = false;
    for file_path in &args.file_paths {
        let rel = if file_path.is_absolute() {
            match file_path.strip_prefix(Path::new(&cache.project_root)) {
                Ok(r) => r.to_string_lossy().into_owned(),
                Err(_) => file_path.to_string_lossy().into_owned(),
            }
        } else {
            file_path.to_string_lossy().into_owned()
        };

        if rescan_one(&mut cache, &rel) {
            warnings = true;
        }
    }

    recompute_stats(&mut cache);
    write_cache(&cache_dir, &cache)?;
    eprintln!("[codeskel] Rescanned {} file(s)", args.file_paths.len());
    Ok(warnings)
}
```

- [ ] **Step 2: Run existing rescan test to verify no regression**

```bash
cargo test test_rescan_updates_coverage -- --nocapture
```

Expected: PASS.

- [ ] **Step 3: Run all tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/commands/rescan.rs
git commit -m "refactor: extract rescan_one and recompute_stats from commands/rescan"
```

---

## Task 3: Add `NextArgs` to `src/cli.rs` and register the command

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add `NextArgs` and `Commands::Next` to `src/cli.rs`**

Add to `Commands` enum (after `Pom`):
```rust
    /// Rescan the last-returned file and return the next file + its dep signatures
    Next(NextArgs),
```

Add struct after `PomArgs`:
```rust
#[derive(Args, Debug)]
pub struct NextArgs {
    /// Path to .codeskel/cache.json (default: .codeskel/cache.json)
    #[arg(long, default_value = ".codeskel/cache.json")]
    pub cache: std::path::PathBuf,
}
```

- [ ] **Step 2: Add `pub mod next` to `src/commands/mod.rs`**

```rust
pub mod next;
```

- [ ] **Step 3: Create stub `src/commands/next.rs` so it compiles**

```rust
use crate::cli::NextArgs;

pub fn run(_args: NextArgs) -> anyhow::Result<bool> {
    todo!("next command not yet implemented")
}
```

- [ ] **Step 4: Add dispatch arm to `src/main.rs`**

```rust
Commands::Next(args) => codeskel::commands::next::run(args),
```

- [ ] **Step 5: Verify it compiles**

```bash
cargo build 2>&1 | head -20
```

Expected: compiles (todo!() is fine at this stage).

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs src/commands/mod.rs src/commands/next.rs src/main.rs
git commit -m "feat: scaffold codeskel next command (stub, not yet implemented)"
```

---

## Task 4: Implement `src/commands/next.rs`

**Files:**
- Modify: `src/commands/next.rs`

- [ ] **Step 1: Write failing integration tests in `tests/integration_scan.rs`**

Append these tests at the end of the file:

```rust
// ── codeskel next tests ──────────────────────────────────────────────

fn make_cache_in(root: &std::path::Path, fixture: &str, tmp: &std::path::Path) {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    codeskel::scanner::scan(&fixture_root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.to_path_buf()),
        verbose: false,
    }).unwrap();
    let _ = root; // unused — fixture_root is the real project_root in cache
}

#[test]
fn test_next_bootstrap_returns_index_0() {
    let tmp = tempdir().unwrap();
    make_cache_in(tmp.path(), "java_project", tmp.path());

    let cache_path = tmp.path().join("cache.json");
    // No session.json yet
    assert!(!tmp.path().join("session.json").exists());

    let args = codeskel::cli::NextArgs { cache: cache_path.clone() };
    let output = codeskel::commands::next::run_and_capture(args).unwrap();

    assert!(!output.done, "bootstrap should not be done");
    assert_eq!(output.index, Some(0), "bootstrap returns index 0");
    assert!(output.file.is_some(), "file must be present");
    assert!(tmp.path().join("session.json").exists(), "session.json must be created");

    let session = codeskel::session::read_session(tmp.path());
    assert_eq!(session.cursor, 0);
}

#[test]
fn test_next_empty_cache_returns_done() {
    use codeskel::models::{CacheFile, Stats};
    use std::collections::HashMap;

    let tmp = tempdir().unwrap();
    // Build an empty-order cache manually
    let cache = CacheFile {
        version: 1,
        scanned_at: "2026-01-01T00:00:00Z".into(),
        project_root: tmp.path().to_string_lossy().into_owned(),
        detected_languages: vec![],
        stats: Stats { total_files: 0, skipped_covered: 0, skipped_generated: 0, to_comment: 0 },
        min_docstring_words: 0,
        order: vec![],
        files: HashMap::new(),
    };
    codeskel::cache::write_cache(tmp.path(), &cache).unwrap();

    let args = codeskel::cli::NextArgs { cache: tmp.path().join("cache.json") };
    let output = codeskel::commands::next::run_and_capture(args).unwrap();

    assert!(output.done, "empty cache → done immediately");
    assert_eq!(output.index, None);
    assert!(output.file.is_none());
    assert!(output.deps.is_empty());
}

#[test]
fn test_next_advance_rescans_and_returns_next() {
    let tmp = tempdir().unwrap();
    make_cache_in(tmp.path(), "java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    // Bootstrap: index 0
    let args0 = codeskel::cli::NextArgs { cache: cache_path.clone() };
    let out0 = codeskel::commands::next::run_and_capture(args0).unwrap();
    assert!(!out0.done);
    assert_eq!(out0.index, Some(0));

    let scanned_before = {
        let cache = codeskel::cache::read_cache(&cache_path).unwrap();
        let rel = &cache.order[0];
        cache.files[rel].scanned_at.clone()
    };

    // Advance: index 1 — should rescan index 0
    std::thread::sleep(std::time::Duration::from_millis(10)); // ensure timestamp advances
    let args1 = codeskel::cli::NextArgs { cache: cache_path.clone() };
    let out1 = codeskel::commands::next::run_and_capture(args1).unwrap();
    assert!(!out1.done || out1.index == Some(1), "second call should advance or reach done");

    let scanned_after = {
        let cache = codeskel::cache::read_cache(&cache_path).unwrap();
        let rel = &cache.order[0];
        cache.files[rel].scanned_at.clone()
    };

    assert_ne!(scanned_before, scanned_after,
        "rescan should have updated scanned_at of index-0 file");
}

#[test]
fn test_next_done_after_last_file() {
    use codeskel::models::{CacheFile, FileEntry, Stats};
    use std::collections::HashMap;

    let tmp = tempdir().unwrap();
    // Single-file cache pointing at a real file we can read
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    codeskel::scanner::scan(&fixture_root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache_path = tmp.path().join("cache.json");
    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let n = cache.order.len();

    // Bootstrap
    let args = codeskel::cli::NextArgs { cache: cache_path.clone() };
    codeskel::commands::next::run_and_capture(args).unwrap();

    // Advance past all remaining files
    let mut last_output = None;
    for _ in 0..n {
        let args = codeskel::cli::NextArgs { cache: cache_path.clone() };
        last_output = Some(codeskel::commands::next::run_and_capture(args).unwrap());
    }

    let done_output = last_output.unwrap();
    assert!(done_output.done, "after n advances past n files, must be done");
    assert_eq!(done_output.index, None);
    assert!(done_output.file.is_none());
    assert_eq!(done_output.remaining, 0);
}
```

- [ ] **Step 2: Add `NextOutput` type and `run_and_capture` to the public API**

Create `src/commands/next.rs`:

```rust
use crate::cache::{read_cache, write_cache};
use crate::cli::NextArgs;
use crate::commands::rescan::{rescan_one, recompute_stats};
use crate::models::FileEntry;
use crate::session::{delete_session, read_session, write_session, Session};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The structured output from a `next` call. Used both for JSON printing and for
/// test assertions (via `run_and_capture`).
#[derive(Debug, Serialize, Deserialize)]
pub struct NextOutput {
    pub done: bool,
    pub index: Option<usize>,
    pub remaining: usize,
    pub file: Option<FileEntry>,
    pub deps: Vec<serde_json::Value>,
}

pub fn run(args: NextArgs) -> anyhow::Result<bool> {
    let output = run_and_capture(args)?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(false)
}

/// Core logic — returns a `NextOutput` instead of printing, so tests can assert on it.
pub fn run_and_capture(args: NextArgs) -> anyhow::Result<NextOutput> {
    let cache_dir = args.cache.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut cache = read_cache(&args.cache)?;
    let session = read_session(&cache_dir);

    // ── Step 1: rescan previous file if session is active ──────────────────
    if session.cursor >= 0 {
        let prev_file = session.current_file.as_deref().unwrap_or("");

        // Sanity check: warn if session drifted from cache.order
        let expected = cache.order.get(session.cursor as usize).map(|s| s.as_str());
        if expected != Some(prev_file) {
            eprintln!(
                "[codeskel] Warning: session mismatch — expected {:?}, found {:?}. Rescanning session file.",
                expected, prev_file
            );
        }

        rescan_one(&mut cache, prev_file);
        recompute_stats(&mut cache);
        write_cache(&cache_dir, &cache)?;
    }

    // ── Step 2: advance cursor ─────────────────────────────────────────────
    let next_cursor = (session.cursor + 1) as usize;

    if next_cursor >= cache.order.len() {
        write_session(&cache_dir, &Session { cursor: -1, current_file: None })?;
        return Ok(NextOutput {
            done: true,
            index: None,
            remaining: 0,
            file: None,
            deps: vec![],
        });
    }

    // ── Step 3: save session and build response ────────────────────────────
    let rel = cache.order[next_cursor].clone();
    write_session(&cache_dir, &Session {
        cursor: next_cursor as i64,
        current_file: Some(rel.clone()),
    })?;

    let file_entry = cache.files.get(&rel)
        .ok_or_else(|| anyhow::anyhow!("File {} in order but missing from files map", rel))?
        .clone();

    let deps: Vec<serde_json::Value> = file_entry.internal_imports.iter()
        .filter_map(|dep| {
            cache.files.get(dep).map(|dep_entry| {
                json!({
                    "path": dep_entry.path,
                    "signatures": dep_entry.signatures,
                })
            })
        })
        .collect();

    let remaining = cache.order.len() - next_cursor - 1;

    Ok(NextOutput {
        done: false,
        index: Some(next_cursor),
        remaining,
        file: Some(file_entry),
        deps,
    })
}
```

- [ ] **Step 3: Run all next tests**

```bash
cargo test test_next -- --nocapture
```

Expected: all 4 tests pass.

- [ ] **Step 5: Run full test suite**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/commands/next.rs tests/integration_scan.rs
git commit -m "feat: implement codeskel next command (bootstrap / advance / done)"
```

---

## Task 5: Delete session on `codeskel scan`

**Files:**
- Modify: `src/commands/scan.rs`

A fresh scan must wipe any existing session so the next loop starts from index 0.

- [ ] **Step 1: Write failing test in `tests/integration_scan.rs`**

Append:

```rust
#[test]
fn test_scan_deletes_session() {
    let tmp = tempdir().unwrap();
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    // First scan + bootstrap to create session.json
    codeskel::scanner::scan(&fixture_root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache_path = tmp.path().join("cache.json");
    let args = codeskel::cli::NextArgs { cache: cache_path.clone() };
    codeskel::commands::next::run_and_capture(args).unwrap();
    assert!(tmp.path().join("session.json").exists(), "session must exist after next");

    // Second scan must delete session.json
    codeskel::scanner::scan(&fixture_root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    assert!(!tmp.path().join("session.json").exists(),
        "session.json must be deleted by scan");
}
```

- [ ] **Step 2: Run to confirm it fails**

```bash
cargo test test_scan_deletes_session -- --nocapture
```

Expected: FAIL — session.json still exists after second scan.

- [ ] **Step 3: Update `src/commands/scan.rs` to delete session**

The `run()` function in `scan.rs` delegates to `codeskel::scanner::scan(...)` which returns a result struct. After a successful scan, add a `delete_session` call. The cache dir comes from `result.cache_path.parent()`.

Add at the top of `scan.rs`:
```rust
use crate::session::delete_session;
```

In `run()`, after `let result = scan(...)?;`, add:
```rust
    if let Some(cache_dir) = result.cache_path.parent() {
        delete_session(cache_dir);
    }
```

- [ ] **Step 4: Check what `scan()` returns — verify `cache_path` field exists**

Look at `src/scanner.rs` for the return type of `scan()`. The field is used in the existing `scan.rs` as `result.cache_path.to_string_lossy()...` — it exists.

- [ ] **Step 5: Run the test to confirm it passes**

```bash
cargo test test_scan_deletes_session -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Run full test suite**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/commands/scan.rs tests/integration_scan.rs
git commit -m "feat: delete session.json on codeskel scan"
```

---

## Task 6: Verify acceptance criteria end-to-end

- [ ] **Step 1: Build release binary**

```bash
cargo build --release
```

Expected: build succeeds.

- [ ] **Step 2: Smoke-test the bootstrap call**

```bash
./target/release/codeskel scan tests/fixtures/java_project --min-coverage 0 2>&1
./target/release/codeskel next
```

Expected:
- `scan` prints JSON summary
- First `next` prints `{ "done": false, "index": 0, "file": {...}, "deps": [...] }`

- [ ] **Step 3: Smoke-test advance and done**

```bash
./target/release/codeskel next   # advance: rescans index 0, returns index 1
./target/release/codeskel next   # done (java_project has 2 files)
```

Expected:
- Second call: `{ "done": false, "index": 1, ... }`
- Third call: `{ "done": true, "remaining": 0, "file": null, "deps": [] }`

- [ ] **Step 4: Smoke-test `--cache` override**

```bash
./target/release/codeskel scan tests/fixtures/java_project --min-coverage 0 --cache-dir /tmp/cs_test 2>&1
./target/release/codeskel next --cache /tmp/cs_test/cache.json
```

Expected: returns first file.

- [ ] **Step 5: Verify rescan command still works (no regression)**

```bash
./target/release/codeskel scan tests/fixtures/java_project --min-coverage 0 2>&1
FIRST=$(./target/release/codeskel get .codeskel/cache.json --index 0 | python3 -c "import sys,json; print(json.load(sys.stdin)['path'])")
./target/release/codeskel rescan .codeskel/cache.json "$FIRST"
```

Expected: `[codeskel] Rescanned 1 file(s)` on stderr, exit 0.

- [ ] **Step 6: Run full test suite one final time**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 7: Add `session.json` to `.gitignore`**

Open `.gitignore` (or `.codeskel/.gitignore` if the project uses per-directory ignores) and append:
```
.codeskel/session.json
```

Commit:
```bash
git add .gitignore
git commit -m "chore: gitignore .codeskel/session.json (transient loop state)"
```

- [ ] **Step 8: Commit smoke-test confirmation (no code changes needed)**

If no code changes were needed beyond the gitignore: nothing to commit. If fixes were made, commit them with appropriate message.
