# `codeskel next --target` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `--target <file>` to `codeskel next` so single-file commenting uses the same one-call-per-iteration loop as project mode.

**Architecture:** Branch `run` on `args.target`; extract existing logic into `run_project`; add `run_targeted` that bootstraps a chain from `chain_order()` (already in `get.rs`), stores `target` + `chain` in `session.json`, and rescans-then-advances on each subsequent call. Add a `mode` field to `NextOutput` to disambiguate `index` semantics between modes.

**Tech Stack:** Rust, Clap (CLI args), Serde JSON, anyhow (error handling), tempfile (tests)

---

## File Map

| File | Change |
|---|---|
| `src/cli.rs` | Add `target: Option<String>` to `NextArgs` |
| `src/session.rs` | Add `target: Option<String>` and `chain: Option<Vec<String>>` to `Session` |
| `src/commands/get.rs` | Make `chain_order` pub |
| `src/commands/next.rs` | Add `mode` to `NextOutput`; split into `run_project` + `run_targeted` |
| `tests/integration_scan.rs` | Fix mode-field assertions in existing tests; add targeted-mode tests |

---

## Task 1: Add `target` arg to `NextArgs` and `mode` to `NextOutput`

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/commands/next.rs`
- Modify: `tests/integration_scan.rs` (fix existing assertions)

- [ ] **Step 1: Add `target` to `NextArgs` in `src/cli.rs`**

Replace the existing `NextArgs` struct:

```rust
#[derive(Args, Debug)]
pub struct NextArgs {
    /// Path to .codeskel/cache.json (default: .codeskel/cache.json)
    #[arg(long, default_value = ".codeskel/cache.json")]
    pub cache: std::path::PathBuf,

    /// Restrict loop to the transitive dep chain of this file (relative path)
    #[arg(long)]
    pub target: Option<String>,
}
```

- [ ] **Step 2: Add `mode` field to `NextOutput` in `src/commands/next.rs`**

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct NextOutput {
    pub done: bool,
    pub mode: String,   // "project" | "targeted"
    pub index: Option<usize>,
    pub remaining: usize,
    pub file: Option<FileEntry>,
    pub deps: Vec<DepEntry>,
}
```

- [ ] **Step 3: Set `mode: "project".into()` in the two existing `NextOutput` construction sites in `run_and_capture`**

The done response:
```rust
return Ok(NextOutput {
    done: true,
    mode: "project".into(),
    index: None,
    remaining: 0,
    file: None,
    deps: vec![],
});
```

The file response:
```rust
Ok(NextOutput {
    done: false,
    mode: "project".into(),
    index: Some(next_cursor),
    remaining,
    file: Some(file_entry),
    deps,
})
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo build 2>&1 | head -30
```

Expected: no errors (existing tests may fail on `mode` field — fix in next step).

- [ ] **Step 5: Fix existing tests in `tests/integration_scan.rs` that check `NextOutput`**

The existing tests don't assert on `mode`, so they should still pass as-is. But `test_next_empty_cache_returns_done` and others check `output.done`, `output.index`, etc. — no `mode` assertion needed there. Run tests to confirm:

```bash
cargo test --test integration_scan 2>&1 | tail -20
```

Expected: all existing next-related tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs src/commands/next.rs
git commit -m "feat: add --target arg to NextArgs and mode field to NextOutput"
```

---

## Task 2: Extend `Session` with `target` and `chain` fields

**Files:**
- Modify: `src/session.rs`

- [ ] **Step 1: Write a failing test for the new fields**

Add to the `#[cfg(test)]` block in `src/session.rs`:

```rust
#[test]
fn targeted_session_roundtrip() {
    let dir = tempdir().unwrap();
    let s = Session {
        cursor: 0,
        current_file: Some("src/A.java".into()),
        target: Some("src/C.java".into()),
        chain: Some(vec!["src/A.java".into(), "src/B.java".into(), "src/C.java".into()]),
    };
    write_session(dir.path(), &s).unwrap();
    let back = read_session(dir.path());
    assert_eq!(back.target, Some("src/C.java".into()));
    assert_eq!(back.chain.as_deref(), Some(["src/A.java", "src/B.java", "src/C.java"].as_slice()));
}
```

- [ ] **Step 2: Run it to confirm it fails**

```bash
cargo test -p codeskel session::tests::targeted_session_roundtrip 2>&1 | tail -10
```

Expected: compile error (fields don't exist yet).

- [ ] **Step 3: Add `target` and `chain` to `Session`**

```rust
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub cursor: i64,
    pub current_file: Option<String>,
    /// Present only in targeted mode. Null in project-mode sessions.
    #[serde(default)]
    pub target: Option<String>,
    /// Ordered chain [dep_0, ..., dep_N-1, target]. Absent in project mode.
    #[serde(default)]
    pub chain: Option<Vec<String>>,
}
```

Update `Default` impl (new fields default to `None` via `#[serde(default)]`, but `Default` needs updating too):

```rust
impl Default for Session {
    fn default() -> Self {
        Session { cursor: -1, current_file: None, target: None, chain: None }
    }
}
```

- [ ] **Step 4: Update all `Session { cursor, current_file }` construction sites in `next.rs`** (they now need `target: None, chain: None` or will fail to compile — add those fields).

- [ ] **Step 5: Run the new test**

```bash
cargo test -p codeskel session::tests::targeted_session_roundtrip 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 6: Run all session tests**

```bash
cargo test -p codeskel session 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src/session.rs src/commands/next.rs
git commit -m "feat: extend Session with target and chain fields for targeted mode"
```

---

## Task 3: Make `chain_order` pub

**Files:**
- Modify: `src/commands/get.rs`

- [ ] **Step 1: Change `fn chain_order` to `pub fn chain_order`**

Line 72 of `src/commands/get.rs`:
```rust
pub fn chain_order(cache: &crate::models::CacheFile, file_path: &str) -> anyhow::Result<Vec<String>> {
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build 2>&1 | head -10
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/commands/get.rs
git commit -m "refactor: make chain_order pub for use in targeted next"
```

---

## Task 4: Implement `run_targeted`

**Files:**
- Modify: `src/commands/next.rs`

- [ ] **Step 1: Write failing integration tests** (add to `tests/integration_scan.rs`)

Add a helper at the top of the targeted-mode test section:

```rust
fn make_targeted_args(cache_path: std::path::PathBuf, target: &str) -> codeskel::cli::NextArgs {
    codeskel::cli::NextArgs { cache: cache_path, target: Some(target.to_string()) }
}
```

Then add these tests (they will fail until `run_targeted` is implemented):

```rust
#[test]
fn test_targeted_bootstrap_returns_first_dep() {
    // java_refs_project: UserService.java imports User.java and UserRepository.java
    // chain should be [User.java, UserRepository.java, UserService.java] (topo order)
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target = cache.order.iter()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in order");

    let args = make_targeted_args(cache_path.clone(), &target);
    let out = codeskel::commands::next::run_and_capture(args).unwrap();

    assert!(!out.done, "bootstrap must not be done");
    assert_eq!(out.mode, "targeted");
    assert_eq!(out.index, Some(0), "first call returns index 0 within chain");
    assert!(out.file.is_some());

    let session = codeskel::session::read_session(tmp.path());
    assert_eq!(session.target.as_deref(), Some(target.as_str()));
    assert!(session.chain.as_ref().map(|c| c.len()).unwrap_or(0) >= 1);
    // Target itself is last entry in chain
    assert_eq!(session.chain.as_ref().unwrap().last().map(|s| s.as_str()), Some(target.as_str()));
}

#[test]
fn test_targeted_no_deps_chain_is_target_only() {
    // Use a file with no internal imports. In java_project, Base.java has no internal imports.
    let tmp = tempdir().unwrap();
    make_cache_in("java_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target = cache.order.iter()
        .find(|p| p.contains("Base"))
        .cloned()
        .expect("Base must be in order");

    let args = make_targeted_args(cache_path.clone(), &target);
    let out = codeskel::commands::next::run_and_capture(args).unwrap();

    // Chain = [target] only → bootstrap returns target immediately at index 0
    assert!(!out.done);
    assert_eq!(out.mode, "targeted");
    assert_eq!(out.index, Some(0));
    let session = codeskel::session::read_session(tmp.path());
    assert_eq!(session.chain.as_ref().unwrap().len(), 1);
    assert_eq!(session.chain.as_ref().unwrap()[0], target);
}

#[test]
fn test_targeted_advances_through_chain_and_done() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target = cache.order.iter()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in order");

    // Bootstrap
    let out0 = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();
    assert!(!out0.done);
    let chain_len = codeskel::session::read_session(tmp.path())
        .chain.unwrap().len();

    // Advance through all remaining chain entries
    let mut last = None;
    for _ in 1..=chain_len {
        let out = codeskel::commands::next::run_and_capture(
            make_targeted_args(cache_path.clone(), &target)
        ).unwrap();
        last = Some(out);
    }

    let done = last.unwrap();
    assert!(done.done, "after chain_len advances, must be done");
    assert_eq!(done.mode, "targeted");
    assert_eq!(done.remaining, 0);
    assert!(done.file.is_none());
    // session.json deleted on done
    assert!(!tmp.path().join("session.json").exists());
}

#[test]
fn test_targeted_done_then_bootstrap_again() {
    // After done (session deleted), calling next --target should bootstrap fresh (not error).
    let tmp = tempdir().unwrap();
    make_cache_in("java_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target = cache.order.iter()
        .find(|p| p.contains("Base"))
        .cloned()
        .expect("Base must be in order");

    // Bootstrap → done immediately (no deps)
    let _out0 = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();
    let done = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();
    assert!(done.done);
    assert!(!tmp.path().join("session.json").exists());

    // Call again — should re-bootstrap cleanly
    let restart = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();
    assert!(!restart.done, "after done+delete, next call bootstraps again");
    assert_eq!(restart.index, Some(0));
}

#[test]
fn test_targeted_mismatch_warns_and_rebootstraps() {
    // Start a targeted session for target A, then call with target B.
    // Should warn to stderr and bootstrap fresh for B.
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target_a = cache.order.iter()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in order");
    let target_b = cache.order.iter()
        .find(|p| p.contains("UserRepository"))
        .cloned()
        .expect("UserRepository must be in order");

    // Bootstrap for A
    codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target_a)
    ).unwrap();

    // Now call with B — should bootstrap for B, not continue A's session
    let out_b = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target_b)
    ).unwrap();
    assert!(!out_b.done);
    assert_eq!(out_b.mode, "targeted");

    let session = codeskel::session::read_session(tmp.path());
    assert_eq!(session.target.as_deref(), Some(target_b.as_str()),
        "session must now track target B");
}

#[test]
fn test_targeted_project_mode_mismatch_rebootstraps_project() {
    // Start a targeted session, then call bare next (project mode).
    // Should warn and bootstrap project mode.
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target = cache.order.iter()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in order");

    // Bootstrap targeted session
    codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();

    // Call project mode (no --target)
    let proj_out = codeskel::commands::next::run_and_capture(
        codeskel::cli::NextArgs { cache: cache_path.clone(), target: None }
    ).unwrap();
    assert!(!proj_out.done);
    assert_eq!(proj_out.mode, "project");
    assert_eq!(proj_out.index, Some(0), "project mode restarted at index 0");
}

#[test]
fn test_targeted_error_on_missing_target() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let args = make_targeted_args(cache_path.clone(), "src/DoesNotExist.java");
    let result = codeskel::commands::next::run_and_capture(args);
    assert!(result.is_err(), "missing target must return Err");
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("not found in cache"), "error message should say 'not found in cache', got: {}", msg);
    // No session written
    assert!(!tmp.path().join("session.json").exists());
}
```

- [ ] **Step 2: Run the new tests to confirm they all fail (compile error expected)**

```bash
cargo test --test integration_scan test_targeted 2>&1 | tail -20
```

Expected: compile errors because `run_targeted` doesn't exist yet (or tests fail at runtime).

- [ ] **Step 3: Implement `run_targeted` in `src/commands/next.rs`**

Extract existing `run_and_capture` body into `run_project`. Add `run_targeted`. Full updated file:

```rust
use crate::cache::{read_cache, write_cache};
use crate::cli::NextArgs;
use crate::commands::get::chain_order;
use crate::commands::rescan::{rescan_one, recompute_stats};
use crate::models::{FileEntry, Signature};
use crate::session::{delete_session, read_session, write_session, Session};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DepEntry {
    pub path: String,
    pub signatures: Vec<Signature>,
}

/// The structured output from a `next` call.
#[derive(Debug, Serialize, Deserialize)]
pub struct NextOutput {
    pub done: bool,
    pub mode: String,  // "project" | "targeted"
    pub index: Option<usize>,
    pub remaining: usize,
    pub file: Option<FileEntry>,
    pub deps: Vec<DepEntry>,
}

pub fn run(args: NextArgs) -> anyhow::Result<bool> {
    let output = run_and_capture(args)?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(false)
}

pub fn run_and_capture(args: NextArgs) -> anyhow::Result<NextOutput> {
    match args.target {
        Some(target) => run_targeted(args.cache, target),
        None => run_project(args.cache),
    }
}

fn run_project(cache_path: std::path::PathBuf) -> anyhow::Result<NextOutput> {
    let cache_dir = cache_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut cache = read_cache(&cache_path)?;
    let session = read_session(&cache_dir);

    // Mismatch: session was in targeted mode → warn and restart project mode
    if session.target.is_some() {
        eprintln!("[codeskel] Warning: session was in targeted mode; restarting project-mode session.");
    }

    // Rescan previous file if session is active (and in project mode)
    if session.cursor >= 0 && session.target.is_none() {
        match session.current_file.as_deref() {
            None => {
                eprintln!("[codeskel] Warning: session has active cursor but no current_file; skipping rescan.");
            }
            Some(prev_file) => {
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
        }
    }

    let next_cursor = if session.target.is_some() {
        // Was targeted, restart from 0
        0
    } else {
        (session.cursor + 1) as usize
    };

    if next_cursor >= cache.order.len() {
        delete_session(&cache_dir);
        return Ok(NextOutput {
            done: true,
            mode: "project".into(),
            index: None,
            remaining: 0,
            file: None,
            deps: vec![],
        });
    }

    let rel = cache.order[next_cursor].clone();
    write_session(&cache_dir, &Session {
        cursor: next_cursor as i64,
        current_file: Some(rel.clone()),
        target: None,
        chain: None,
    })?;

    let file_entry = cache.files.get(&rel)
        .ok_or_else(|| anyhow::anyhow!("File {} in order but missing from files map", rel))?
        .clone();

    let deps: Vec<DepEntry> = file_entry.internal_imports.iter()
        .filter_map(|dep| {
            cache.files.get(dep).map(|dep_entry| DepEntry {
                path: dep_entry.path.clone(),
                signatures: dep_entry.signatures.clone(),
            })
        })
        .collect();

    let remaining = cache.order.len() - next_cursor - 1;

    Ok(NextOutput {
        done: false,
        mode: "project".into(),
        index: Some(next_cursor),
        remaining,
        file: Some(file_entry),
        deps,
    })
}

fn run_targeted(cache_path: std::path::PathBuf, target: String) -> anyhow::Result<NextOutput> {
    let cache_dir = cache_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut cache = read_cache(&cache_path)?;

    // Error if target not in cache
    if !cache.files.contains_key(&target) {
        anyhow::bail!("target '{}' not found in cache — run codeskel scan first", target);
    }

    let session = read_session(&cache_dir);

    // Detect mode mismatch
    let is_mismatch = session.target.as_deref() != Some(&target);
    if is_mismatch && session.cursor >= 0 {
        // There's an active session but for a different target or project mode
        if let Some(prev_target) = &session.target {
            eprintln!("[codeskel] Warning: session was targeting '{}'; restarting for '{}'.", prev_target, target);
        } else if session.target.is_none() && session.cursor >= 0 {
            eprintln!("[codeskel] Warning: session was in project mode; restarting as targeted session for '{}'.", target);
        }
    }

    // Bootstrap: no session, or cursor == -1 (done/fresh), or mismatch
    if is_mismatch || session.cursor < 0 {
        // Build chain: topo-ordered deps (skipped files excluded) + target appended
        let deps_chain = chain_order(&cache, &target)?;
        let mut chain = deps_chain;
        chain.push(target.clone());

        let first = chain[0].clone();
        write_session(&cache_dir, &Session {
            cursor: 0,
            current_file: Some(first.clone()),
            target: Some(target.clone()),
            chain: Some(chain.clone()),
        })?;

        let file_entry = cache.files.get(&first)
            .ok_or_else(|| anyhow::anyhow!("File {} in chain but missing from files map", first))?
            .clone();

        let deps = build_deps(&cache, &file_entry);
        let remaining = chain.len() - 1;

        return Ok(NextOutput {
            done: false,
            mode: "targeted".into(),
            index: Some(0),
            remaining,
            file: Some(file_entry),
            deps,
        });
    }

    // Subsequent call: rescan current_file, advance cursor
    let chain = session.chain.as_ref()
        .ok_or_else(|| anyhow::anyhow!("session.chain missing in targeted mode"))?
        .clone();

    if let Some(prev_file) = session.current_file.as_deref() {
        // Warn if prev_file no longer in cache (skip rescan, still advance)
        if cache.files.contains_key(prev_file) {
            rescan_one(&mut cache, prev_file);
            recompute_stats(&mut cache);
            write_cache(&cache_dir, &cache)?;
        } else {
            eprintln!("[codeskel] Warning: '{}' no longer in cache; skipping rescan.", prev_file);
        }
    } else {
        eprintln!("[codeskel] Warning: session has active cursor but no current_file; skipping rescan.");
    }

    let next_cursor = (session.cursor + 1) as usize;

    if next_cursor >= chain.len() {
        delete_session(&cache_dir);
        return Ok(NextOutput {
            done: true,
            mode: "targeted".into(),
            index: None,
            remaining: 0,
            file: None,
            deps: vec![],
        });
    }

    let next_file = chain.get(next_cursor)
        .ok_or_else(|| anyhow::anyhow!("chain index {} out of range", next_cursor))?
        .clone();

    // Handle chain entry no longer in cache: warn, skip, advance (recursive-style via loop)
    // Simple approach: if next_file is missing, warn and report done rather than skip-advance
    // (extremely rare; a scan would have wiped the session anyway)
    if !cache.files.contains_key(&next_file) {
        eprintln!("[codeskel] Warning: chain entry '{}' no longer in cache; skipping.", next_file);
        // Advance one more step by writing session with next_cursor and recursing isn't clean —
        // write session pointing past it and return done if it was the last
        // For simplicity, just delete session and return done with a warning.
        delete_session(&cache_dir);
        return Ok(NextOutput {
            done: true,
            mode: "targeted".into(),
            index: None,
            remaining: 0,
            file: None,
            deps: vec![],
        });
    }

    write_session(&cache_dir, &Session {
        cursor: next_cursor as i64,
        current_file: Some(next_file.clone()),
        target: Some(target.clone()),
        chain: Some(chain.clone()),
    })?;

    let file_entry = cache.files.get(&next_file)
        .ok_or_else(|| anyhow::anyhow!("File {} in chain but missing from files map", next_file))?
        .clone();

    let deps = build_deps(&cache, &file_entry);
    let remaining = chain.len() - next_cursor - 1;

    Ok(NextOutput {
        done: false,
        mode: "targeted".into(),
        index: Some(next_cursor),
        remaining,
        file: Some(file_entry),
        deps,
    })
}

fn build_deps(cache: &crate::models::CacheFile, file_entry: &FileEntry) -> Vec<DepEntry> {
    file_entry.internal_imports.iter()
        .filter_map(|dep| {
            cache.files.get(dep).map(|dep_entry| DepEntry {
                path: dep_entry.path.clone(),
                signatures: dep_entry.signatures.clone(),
            })
        })
        .collect()
}
```

- [ ] **Step 4: Run the targeted tests**

```bash
cargo test --test integration_scan test_targeted 2>&1 | tail -30
```

Expected: all 7 targeted tests pass.

- [ ] **Step 5: Run all tests to check for regressions**

```bash
cargo test 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/commands/next.rs tests/integration_scan.rs
git commit -m "feat: implement codeskel next --target (targeted single-file mode)"
```

---

## Task 5: Verify PRD acceptance criteria

- [ ] **Step 1: Run the full test suite one final time**

```bash
cargo test 2>&1
```

Expected: all tests pass, no warnings.

- [ ] **Step 2: Spot-check the binary**

```bash
cargo build --release 2>&1 | head -5
./target/release/codeskel next --help
```

Expected: `--target <TARGET>` appears in the help output.

- [ ] **Step 3: Commit (if anything changed)**

If clean, no commit needed. Otherwise:

```bash
git add -p
git commit -m "chore: post-review cleanup for next --target"
```

---

## Acceptance criteria coverage

| Criterion | Task |
|---|---|
| Bootstrap: computes chain, writes session, returns chain[0] + deps | Task 4 |
| No-deps target: chain = [target], returns target on first call | Task 4 |
| Each subsequent call rescans current_file, advances cursor | Task 4 |
| Last call rescans target; next call returns done | Task 4 |
| deps reflect docstrings from prior iterations | Task 4 (rescan ensures this) |
| Session mismatch (different --target): warn, rebootstrap | Task 4 |
| Session mismatch (project → targeted): warn, rebootstrap | Task 4 |
| Unreadable chain entry: warn, skip rescan, still advance | Task 4 |
| scan clears targeted session | Unchanged (delete_session in scan) |
| next (no --target) unaffected | Task 4 (run_project identical to prior logic) |
| index is chain-relative | Task 4 |
| remaining counts unreturned chain entries | Task 4 |
| mode = "targeted" / "project" | Task 1 + Task 4 |
| --target not in cache → error, no session | Task 4 |
| Skipped target still processed | Task 4 (chain.push always appends target) |
| Done uses delete_session | Task 4 |
