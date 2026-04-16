# PRD: `codeskel next --target <file>` — targeted single-file mode

## Background

`codeskel next` (shipped in the `next` command PRD) reduced the project-mode commenting loop
from 3 calls per file to 1. The comment skill now drives the full-project loop cleanly:

```
scan → loop { next → comment → (next rescans automatically) } → done
```

Single-file mode — commenting one target file and the subset of deps it actually uses — still
requires the old manual sequence:

```
scan
get --chain <target>                          # chain size
get --refs <target>                           # refs map
for i in 0..N:
  get --chain <target> --index <i>            # dep details
  get --deps <dep>                            # dep's own dep signatures
  [comment only referenced symbols]
  rescan <cache> <dep>                        # flush to cache
get --deps <target>                           # target context
[comment target]
rescan <cache> <target>
```

That is 4 + 3×N shell calls, plus manual cursor tracking. The same problems that motivated
`next` for project mode exist here: `rescan` is structurally optional so it gets skipped or
deferred, and the batching antipattern degrades docstring quality.

## Goal

Extend `codeskel next` with a `--target <file>` flag that restricts the loop to the
transitive dependency chain of a single target file, processed in topo order (deepest dep
first), with the target file itself returned last.

After this change, single-file mode uses the same one-call-per-iteration loop as project
mode. The comment skill's per-iteration body stays unchanged; only the upfront `refs_map`
build and the symbol-filtering logic remain skill-side (they are semantic, not structural).

## Non-goals

- Does not embed refs filtering into `codeskel` — deciding which symbols need docstrings
  is a concern for the LLM, not the CLI
- Does not change the behavior of `codeskel next` without `--target`
- Does not replace `codeskel get --chain`, `--refs`, or `--deps` (still useful ad-hoc)
- Does not support multiple targets in one session

---

## Command specification

### Syntax

```
codeskel next --target <FILE> [--cache <CACHE_PATH>]
```

| Option | Default | Description |
|---|---|---|
| `--target <FILE>` | — | Relative path to the file being targeted. Restricts the loop to its transitive dep chain + itself. |
| `--cache <CACHE_PATH>` | `.codeskel/cache.json` | Path to the cache file. |

`--target` and the bare `codeskel next` (project mode) are mutually exclusive per session.
Running `codeskel next --target A` after a project-mode session (or vice versa) triggers a
session mismatch warning and starts a fresh targeted session.

### Session state

Targeted mode extends `session.json` with two additional fields:

```json
{
  "cursor": 1,
  "current_file": "src/main/java/com/example/model/Address.java",
  "target": "src/main/java/com/example/service/UserService.java",
  "chain": [
    "src/main/java/com/example/base/Entity.java",
    "src/main/java/com/example/model/Address.java",
    "src/main/java/com/example/model/User.java",
    "src/main/java/com/example/service/UserService.java"
  ]
}
```

| Field | Description |
|---|---|
| `target` | Relative path of the target file. Null in project-mode sessions. |
| `chain` | Ordered list of files to process: `[dep_0, dep_1, ..., dep_N-1, target]`. Computed once on bootstrap from `--chain` and stored so subsequent calls require no re-derivation. The target itself is the last entry. |

`chain` is computed on the bootstrap call as:
```
deps = codeskel_get_chain(target)   // topo order, deepest first
chain = deps + [target]
```

This mirrors the topo order that `get --chain --index i` returns today.

### Behavior

**Bootstrap call** (no session, or session has no `target`, or `target` field differs):
- Derive `chain` from the cache's topo-sorted dep list for `--target`
- If `chain` is empty (target has no internal deps), still add the target itself as the only entry
- Write session: `{ cursor: 0, current_file: chain[0], target: <target>, chain: [...] }`
- Return chain[0] + its dep signatures (same output schema as project-mode `next`)

**Bootstrap call — error case**: If `--target` is not present in `cache.files`, exit with a
clear error message (e.g. `"target '<file>' not found in cache — run codeskel scan first"`).
No session is written.

**Bootstrap call — skipped target**: If the target is in `cache.files` but not in
`cache.order` (already fully commented), it is still appended to `chain` as the last entry
and processed normally. The LLM can decide to skip it if coverage is already sufficient.

**Subsequent calls** (session has matching `target`):
1. Rescan `session.current_file` (same logic as project-mode `next`)
2. Write updated `cache.json`
3. Advance: `next_cursor = session.cursor + 1`
4. If `next_cursor >= session.chain.len()` → call `delete_session`, output done response
5. Otherwise → write new session, output file response for `session.chain[next_cursor]`

**When the target file is returned** (last entry in `chain`):
- `file` is the target itself
- `deps` contains its full direct dep signatures (now fully up-to-date, since all chain deps
  were rescanned in earlier iterations)
- The LLM comments the target in full (all `has_docstring: false` items, no refs filtering)
- On the *next* `next --target` call, the target is rescanned and `done: true` is returned

### Output format

Adds one new field (`mode`) to `NextOutput` to disambiguate index semantics.

> **Note on `index`:** In project mode `index` is the 0-based position in `cache.order`
> (global). In targeted mode `index` is the 0-based position within `chain` (local). The
> `mode` field makes this unambiguous for any consumer that handles both modes.

**File response (targeted mode):**
```json
{
  "done": false,
  "mode": "targeted",
  "index": 1,
  "remaining": 2,
  "file": {
    "path": "src/main/java/com/example/model/Address.java",
    "language": "java",
    "comment_coverage": 0.05,
    "skip": false,
    "cycle_warning": false,
    "internal_imports": ["src/main/java/com/example/base/Entity.java"],
    "signatures": [ ... ]
  },
  "deps": [
    {
      "path": "src/main/java/com/example/base/Entity.java",
      "signatures": [ ... ]
    }
  ]
}
```

`index` is the 0-based position within `chain` (not the global cache order).
`remaining` counts entries in `chain` not yet returned.

Project-mode responses emit `"mode": "project"` with `index` as the global `cache.order`
position (unchanged from the existing implementation).

**Done response:**
```json
{ "done": true, "index": null, "remaining": 0, "file": null, "deps": [] }
```

### Loop pattern — updated single-file skill workflow

```bash
# Step 1: scan (unchanged)
codeskel scan <project_root>

# Step 2: build refs_map upfront (skill-side, unchanged)
codeskel get <cache> --refs <target>
# repeat for each chain file if needed

# Step 3: loop using next --target
codeskel next --target <target>
# → { done: false, index: 0, file: { path: "base/Entity.java", ... }, deps: [...] }

# [LLM comments only symbols where has_docstring:false AND name ∈ refs_map[file.path]]

codeskel next --target <target>
# → { done: false, index: 1, file: { path: "model/User.java", ... }, deps: [...] }

# ... repeat through deps ...

codeskel next --target <target>
# → { done: false, index: N, file: { path: "<target>", ... }, deps: [...] }

# [LLM comments target in full — no refs filtering needed]

codeskel next --target <target>
# → { done: true }
```

Call count: `2 + chain.len() + 1` (scan + refs calls + loop + final done).
Previously: `4 + 3×N`.

---

## Session mode conflicts and lifecycle

| Event | Behavior |
|---|---|
| `codeskel scan` run | Deletes `session.json`; next `next` call bootstraps fresh regardless of `--target` |
| `next --target A` after `next` (project mode) | Session `target` is null → mismatch; warn and bootstrap new targeted session for A |
| `next` (project mode) after `next --target A` | Session has `target` set → mismatch; warn and bootstrap fresh project-mode session |
| `next --target A` after `next --target B` | `target` mismatch; warn and bootstrap new targeted session for A |
| `session.current_file` unreadable | Warn, skip rescan, still advance |
| `session.chain` entry no longer in cache | Warn to stderr, skip that entry, advance to next |
| `--target` file not in `cache.files` | Error with message; no session written |

Warnings go to stderr; stdout always contains valid JSON.

---

## SKILL.md changes (comment skill)

After this feature ships, the single-file workflow in `SKILL.md` simplifies to:

**Step 1 — Scan** (unchanged)

**Step 2 — Build refs map** (unchanged — skill-side semantic filtering)

```bash
codeskel get <cache> --refs <target>
# optionally: repeat for each file in the chain if cross-chain refs are needed
```

**Step 3 — Loop using `next --target`**

Repeat until `done` is `true`:

```bash
codeskel next --target <target> [--cache <cache_path>]
```

- If `result.file.path == target`: comment all items where `has_docstring: false` (full pass, no refs filter)
- Otherwise: comment only items where `has_docstring: false` AND `name ∈ refs_map[result.file.path]`
- If no items match for a dep file, skip silently (still call `next` to trigger rescan)

Print progress: `[dep <index>/<chain_len-1>] <file.path>` for deps; `[target] <file.path>` for the last entry.

---

## Implementation guide

### 1. Extend `NextArgs` in `src/cli.rs`

```rust
#[derive(Args, Debug)]
pub struct NextArgs {
    #[arg(long, default_value = ".codeskel/cache.json")]
    pub cache: std::path::PathBuf,

    /// Restrict loop to the transitive dep chain of this file (relative path)
    #[arg(long)]
    pub target: Option<String>,
}
```

### 2. Extend `Session` in `src/session.rs`

```rust
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Session {
    pub cursor: i64,
    pub current_file: Option<String>,
    /// Present only in targeted mode
    pub target: Option<String>,
    /// Ordered chain: [dep_0, ..., dep_N-1, target]. Absent in project mode.
    pub chain: Option<Vec<String>>,
}
```

### 3. Add `mode` field to `NextOutput` in `src/commands/next.rs`

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

Branch `run` on `args.target`:

```rust
pub fn run(args: NextArgs) -> anyhow::Result<bool> {
    match args.target {
        Some(target) => run_targeted(args.cache, target),
        None => run_project(args.cache),   // existing logic, extracted
    }
}
```

`run_targeted`:

1. Load cache and session
2. **Error if target not in `cache.files`**: bail with `"target '<file>' not found in cache — run codeskel scan first"`
3. Detect mode mismatch (session.target != Some(&target)): if mismatch, warn stderr and bootstrap
4. On bootstrap: derive chain via `chain_order(&cache, &target)` (from `get.rs`), append target (even if target is not in `cache.order`), write session
5. On subsequent calls: rescan `session.current_file`, write cache, advance cursor
6. If exhausted: call `delete_session` (not write cursor:-1), print done response
7. Otherwise: fetch `session.chain[next_cursor]` from cache, build deps, print file response with `mode: "targeted"`

`chain_order` is already in `src/commands/get.rs` — make it `pub` and call it directly (no need to move to `graph.rs`).

Project-mode `run_project` emits `mode: "project"` in its `NextOutput`.

### 4. No changes to `scan`, `get`, or `rescan`

### 5. No changes to `scan`, `get`, or `rescan`

Session cleanup on scan (`delete_session`) already handles targeted sessions since it
unconditionally removes `session.json`.

---

## Acceptance criteria

- [ ] `codeskel next --target <file>` (no session) bootstraps: computes chain, writes session, returns chain[0] + deps
- [ ] `codeskel next --target <file>` when target has no internal deps: chain = [target], returns target on first call
- [ ] Each subsequent call rescans `session.current_file`, advances cursor, returns next chain entry
- [ ] Last call before done rescans target file and returns `done: true`
- [ ] `deps` for each chain entry reflect docstrings written in all prior iterations
- [ ] `deps` for the target file reflects freshly-rescanned chain deps
- [ ] Session mismatch (different `--target`) emits warning to stderr, bootstraps new targeted session
- [ ] Session mismatch (project mode → targeted) emits warning, bootstraps targeted session
- [ ] Unreadable chain entry emits warning, skips rescan, still advances
- [ ] `codeskel scan` clears targeted session (same as project mode)
- [ ] `codeskel next` (no `--target`) is unaffected by this change
- [ ] `index` in output is chain-relative (0-based position within `chain`)
- [ ] `remaining` counts unreturned chain entries (0 when done)
- [ ] `mode` field is `"targeted"` in targeted-mode responses and `"project"` in project-mode responses
- [ ] `--target` pointing to a file not in `cache.files` exits with a clear error; no session written
- [ ] `--target` pointing to a file in `cache.files` but not `cache.order` (skipped) still processes it as the last chain entry
- [ ] Done call uses `delete_session` (not writing `cursor: -1`); subsequent call after done bootstraps fresh
