# Compact Output Design

**Date:** 2026-04-16  
**Status:** Approved

## Problem

All commands use `serde_json::to_string_pretty`, which produces verbose indented JSON. The output is consumed by Claude Code, not humans. The `next` command — used on every iteration of the commenting loop — includes redundant fields that add tokens without adding value.

## Goals

1. Reduce token usage across all commands by switching to compact JSON.
2. Make `next` output signal-dense: remove fields Claude never uses, filter dep signatures to only what the current file references.

## Design

### 1. Compact JSON everywhere

Change every command's `run()` from `serde_json::to_string_pretty` to `serde_json::to_string`. Affects: `scan`, `get`, `next`, `pom`. `rescan` prints only to `eprintln!` (no JSON output) — no change needed there.

Expected ~3-4x token reduction with no behavioral change.

### 2. `next` output structs

#### Updated `NextOutput`

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct NextOutput {
    pub done: bool,
    pub mode: String,              // "project" | "targeted"
    pub index: Option<usize>,
    pub remaining: usize,
    pub file: Option<NextFileEntry>,   // was Option<FileEntry>
    pub deps: Vec<DepEntry>,
}
```

#### New `NextFileEntry`

Replaces `FileEntry` in `NextOutput.file`. Defined in `src/commands/next.rs`:

```rust
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
```

Fields removed vs `FileEntry`:

| Removed field | Reason |
|---|---|
| `skip` | Always false in the loop — skip files are excluded from `order` |
| `internal_imports` | Redundant — dep paths already appear in `deps[].path` |
| `skip_reason` | Internal bookkeeping, not useful to Claude |
| `scanned_at` | Internal bookkeeping, not useful to Claude |

### 3. `deps` — `DepSignature` struct + refs-filtered content

#### Updated `DepEntry`

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct DepEntry {
    pub path: String,
    pub signatures: Vec<DepSignature>,   // was Vec<Signature>
}
```

#### New `DepSignature`

Defined in `src/commands/next.rs`:

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
}
```

Fields removed vs `Signature`:

| Removed field | Reason |
|---|---|
| `has_docstring` | Claude does not need to document dep symbols |
| `line` | Claude will not navigate to lines in dep files |

`docstring_text` is kept: when a dep symbol is already documented, Claude reads that text instead of opening the dep file.

#### Refs-based filtering in `build_deps`

`build_deps` becomes fallible and calls `compute_refs` (already in `src/commands/get.rs`) once for the current file, producing a `HashMap<dep_path, Vec<String>>`. Filtering rules applied per dep:

**Key lookup semantics:**
- `dep_path` **absent from the map** → refs analysis produced no entry for that dep → fallback: include all signatures
- `dep_path` **present with empty Vec** → dep was scanned but no symbols matched → fallback: include all signatures
- `dep_path` **present with non-empty Vec** → filter applies

**When filter applies:**
- Top-level type declarations (`kind` ∈ `{class, interface, enum, struct, trait, type_alias}`): always included as structural anchors
- Other signatures: included only if `name` ∈ the refs Vec for that dep

**Fallback scope:** `compute_refs` does one source-file read. If that read fails (I/O error), log a warning via `eprintln!` and skip refs analysis entirely — all deps fall back to unfiltered signatures. This is a single decision affecting all deps for that call.

**Signature conversion:** All included `Signature` values are converted to `DepSignature` (drop `has_docstring` and `line`).

**Call sites:** Both `run_project` and `run_targeted` call `build_deps`. Since `build_deps` becomes `fn build_deps(cache: &CacheFile, file_entry: &FileEntry) -> anyhow::Result<Vec<DepEntry>>`, both call sites propagate with `?`. The enclosing `run_and_capture` already returns `anyhow::Result<NextOutput>`, so this is a natural fit.

## File Changes

| File | Change |
|---|---|
| `src/commands/scan.rs` | `to_string_pretty` → `to_string` |
| `src/commands/get.rs` | `to_string_pretty` → `to_string` |
| `src/commands/pom.rs` | `to_string_pretty` → `to_string` |
| `src/commands/next.rs` | Add `NextFileEntry`, `DepSignature`; update `NextOutput`, `DepEntry`; update `build_deps` |
| `src/commands/rescan.rs` | No change (prints only to `eprintln!`, no JSON output) |
| `src/models.rs` | No changes |

## Non-Goals

- No changes to `get`, `scan`, `pom` output structure — only serialization format.
- No changes to cache format or session format.
- No new CLI flags.
