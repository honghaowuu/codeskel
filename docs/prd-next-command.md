# PRD: `codeskel next` command

## Background

The comment skill drives Claude Code through a file-by-file commenting loop. Each iteration currently requires three separate shell calls:

```
codeskel get  <cache> --index <i>      # fetch file details
codeskel get  <cache> --deps  <file>   # fetch dep signatures
codeskel rescan <cache> <file>         # flush docstrings back into cache
```

The `rescan` step is the critical one: it re-parses the just-written file and writes updated signatures into the cache, so the *next* file's `--deps` response contains freshly-written docstrings rather than stale stubs. Without it, downstream files lose cross-file context and produce lower-quality docstrings.

In practice, Claude Code tends to batch all `get` calls upfront and defer or skip `rescan`, defeating the incremental design. The root cause: `rescan` is structurally optional (a separate fire-and-forget command) so there is no forcing function.

## Goal

Add a `codeskel next` command that makes the rescan structurally unavoidable by fusing all three per-iteration operations into one atomic call:

1. Rescan the file last returned (flush its new docstrings to cache)
2. Advance the cursor to the next file
3. Return that file's details + dep signatures in a single JSON response

This reduces the per-iteration call count from 3 → 1, requires **zero arguments** after the bootstrap, and makes it impossible to advance without triggering a rescan.

## Non-goals

- Does not replace `codeskel scan` (initial scan is unchanged)
- Does not replace `codeskel get` (still needed for single-file mode chain/refs queries)
- Does not replace `codeskel rescan` (still useful for ad-hoc re-parsing outside the loop)
- Does not add a file-watcher or daemon

---

## Command specification

### Syntax

```
codeskel next [--cache <CACHE_PATH>]
```

| Option | Default | Description |
|---|---|---|
| `--cache <CACHE_PATH>` | `.codeskel/cache.json` | Path to the cache file. Explicit path needed only for non-standard locations or multi-project setups. |

No file path argument. The command derives what to rescan from the session state (see below).

### Session state file

`codeskel next` persists a small session file alongside the cache:

**`.codeskel/session.json`**
```json
{
  "cursor": 2,
  "current_file": "src/main/java/com/example/model/User.java"
}
```

| Field | Description |
|---|---|
| `cursor` | 0-based index of the file currently being processed (the one Claude was last told to comment) |
| `current_file` | Path of that file (sanity-check against `cache.order[cursor]`) |

**Lifecycle:**
- Created / updated on every `codeskel next` call
- **Deleted by `codeskel scan`** — a fresh scan always starts a fresh loop
- Should be added to `.gitignore` (transient, not part of the scan artifact)

### Behavior

**Bootstrap call** (no session file, or `cursor == -1`):
- Do not rescan anything
- Write session: `{ "cursor": 0, "current_file": cache.order[0] }`
- Return the file at `cache.order[0]` plus its dep signatures

**Subsequent calls** (session file exists with `cursor >= 0`):
1. Read `session.current_file` — warn if it doesn't match `cache.order[session.cursor]` (cache may have been rescanned externally)
2. Re-parse `session.current_file` from disk (same logic as `rescan`)
3. Update its entry in the cache (signatures, `comment_coverage`, `scanned_at`)
4. Recompute `cache.stats` and write updated `cache.json`
5. Advance: `next_cursor = session.cursor + 1`
6. If `next_cursor >= cache.order.len()` → write `{ "cursor": -1 }` to session, output **done response**, exit 0
7. Otherwise → write `{ "cursor": next_cursor, "current_file": cache.order[next_cursor] }` to session, output **file response**

### Output format

**File response** (more files remain):
```json
{
  "done": false,
  "index": 3,
  "remaining": 8,
  "file": {
    "path": "src/main/java/com/example/service/UserService.java",
    "language": "java",
    "comment_coverage": 0.12,
    "skip": false,
    "cycle_warning": false,
    "internal_imports": ["src/main/java/com/example/model/User.java"],
    "signatures": [ ... ]
  },
  "deps": [
    {
      "path": "src/main/java/com/example/model/User.java",
      "signatures": [ ... ]
    }
  ]
}
```

**Done response** (loop complete):
```json
{
  "done": true,
  "index": null,
  "remaining": 0,
  "file": null,
  "deps": []
}
```

Fields:

| Field | Type | Description |
|---|---|---|
| `done` | bool | `true` when no more files remain |
| `index` | int \| null | 0-based position of the returned file in `cache.order` |
| `remaining` | int | Files left after the returned one (0 when done) |
| `file` | FileEntry \| null | Full file entry (same schema as `get --index` today) |
| `deps` | array | Dep signature summaries (same schema as `get --deps` today) |

### Loop pattern (updated skill workflow)

```bash
# Bootstrap — no rescan, writes session {cursor:0}, returns index 0
codeskel next
# → { "done": false, "index": 0, "file": { "path": "src/model/User.java", ... }, "deps": [...] }

# [Claude comments src/model/User.java and writes it back]

# Advance — rescans User.java (from session), writes session {cursor:1}, returns index 1
codeskel next
# → { "done": false, "index": 1, "file": { "path": "src/service/UserService.java", ... }, "deps": [...] }

# [Claude comments UserService.java and writes it back]

# Advance — rescans UserService.java, cursor→2, ...
codeskel next
# → { "done": false, "index": 2, ... }

# ... repeat until:
# → { "done": true, ... }
```

### Edge cases

| Situation | Behavior |
|---|---|
| No session file (first run after scan) | Treated as bootstrap: start at index 0, no rescan |
| `session.current_file` doesn't match `cache.order[cursor]` | Warn to stderr: `"Session file mismatch — expected X, found Y. Rescanning Y."` Rescan `session.current_file` anyway, then advance |
| `session.current_file` not readable from disk | Warn to stderr, skip rescan for that file, still advance and return next |
| `cache.order` is empty | Bootstrap returns done response immediately |
| `--cache` path not found | Exit non-zero with error (same as today) |
| `codeskel scan` run mid-loop | Scan deletes `session.json`; next `codeskel next` bootstraps from index 0 |

---

## Implementation guide

### 1. Update `src/cli.rs`

Add `--cache` as an optional argument with a default, and add `Next`:

```rust
/// Rescan the last-returned file and return the next file + its dep signatures
Next(NextArgs),
```

```rust
#[derive(Args, Debug)]
pub struct NextArgs {
    /// Path to .codeskel/cache.json (default: .codeskel/cache.json)
    #[arg(long, default_value = ".codeskel/cache.json")]
    pub cache: std::path::PathBuf,
}
```

> **Note:** apply the same `--cache` default to `get` and `rescan` for consistency (separate, low-risk change).

### 2. Add session helpers (e.g. `src/session.rs`)

```rust
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Session {
    pub cursor: i64,           // -1 = not started / complete
    pub current_file: Option<String>,
}

pub fn read_session(cache_dir: &Path) -> Session { ... }
pub fn write_session(cache_dir: &Path, session: &Session) -> anyhow::Result<()> { ... }
pub fn delete_session(cache_dir: &Path) { ... }   // called by scan
```

### 3. Create `src/commands/next.rs`

```rust
pub fn run(args: NextArgs) -> anyhow::Result<bool> {
    let cache_dir = args.cache.parent()...;
    let mut cache = read_cache(&args.cache)?;
    let session = read_session(&cache_dir);

    // Step 1: rescan previous file if session is active
    if session.cursor >= 0 {
        let prev_file = session.current_file.as_deref().unwrap_or("");
        // sanity check
        if cache.order.get(session.cursor as usize).map(|s| s.as_str()) != Some(prev_file) {
            eprintln!("[codeskel] Warning: session mismatch ...");
        }
        rescan_one(&mut cache, prev_file)?;
        recompute_stats(&mut cache);
        write_cache(&cache_dir, &cache)?;
    }

    // Step 2: advance cursor
    let next_cursor = (session.cursor + 1) as usize;

    if next_cursor >= cache.order.len() {
        write_session(&cache_dir, &Session { cursor: -1, current_file: None })?;
        return print_done();
    }

    // Step 3: save session and return next file
    let rel = cache.order[next_cursor].clone();
    write_session(&cache_dir, &Session {
        cursor: next_cursor as i64,
        current_file: Some(rel.clone()),
    })?;

    let file_entry = cache.files.get(&rel)...;
    let deps = build_deps(&cache, &rel);
    let remaining = cache.order.len() - next_cursor - 1;

    println!("{}", serde_json::to_string_pretty(&json!({
        "done": false,
        "index": next_cursor,
        "remaining": remaining,
        "file": file_entry,
        "deps": deps,
    }))?);

    Ok(false)
}
```

Extract the per-file rescan logic from `commands/rescan.rs` into `fn rescan_one(cache: &mut CacheFile, rel: &str) -> anyhow::Result<bool>` so both `rescan` and `next` share the same code path.

### 4. Update `commands/scan.rs`

Call `delete_session(&cache_dir)` at the end of a successful scan so stale cursors never survive a rescan.

### 5. Register

In `src/commands/mod.rs`:
```rust
pub mod next;
```

In `src/main.rs`:
```rust
Commands::Next(args) => commands::next::run(args),
```

---

## Acceptance criteria

- [ ] `codeskel next` (no args, no session) returns `done: false` with index 0 file + deps
- [ ] `codeskel next` returns `done: true` immediately when `cache.order` is empty
- [ ] `codeskel next` rescans `session.current_file`, advances cursor, returns next file + deps
- [ ] `codeskel next` on the last file rescans it, writes cache, returns `done: true`
- [ ] `deps` array contains fresh docstring text from already-rescanned dependency files
- [ ] `--cache <path>` overrides the default `.codeskel/cache.json`
- [ ] Session mismatch emits a warning but does not abort; rescans `session.current_file`
- [ ] Unreadable `session.current_file` emits a warning, skips rescan, still advances
- [ ] `codeskel scan` deletes `session.json` on success
- [ ] `codeskel rescan` continues to work unchanged (no regression)
