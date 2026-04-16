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

Change every command's `run()` from `serde_json::to_string_pretty` to `serde_json::to_string`. Affects: `scan`, `get`, `next`, `pom`, `rescan`. No behavioral change. Expected ~3-4x token reduction.

### 2. `next` file entry — `NextFileEntry` struct

Replace `FileEntry` in `NextOutput.file` with a new `NextFileEntry` (defined in `src/commands/next.rs`):

```rust
pub struct NextFileEntry {
    pub path: String,
    pub language: String,
    pub package: Option<String>,          // skip_serializing_if None
    pub comment_coverage: f64,
    pub cycle_warning: bool,              // skip_serializing_if false
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

#### 3a. New `DepSignature` type

Replace `Vec<Signature>` in `DepEntry` with `Vec<DepSignature>` (defined in `src/commands/next.rs`):

```rust
pub struct DepSignature {
    pub kind: String,
    pub name: String,
    pub modifiers: Vec<String>,           // skip_serializing_if empty
    pub params: Option<Vec<Param>>,       // skip_serializing_if None
    pub return_type: Option<String>,      // skip_serializing_if None
    pub throws: Vec<String>,              // skip_serializing_if empty
    pub extends: Option<String>,          // skip_serializing_if None
    pub implements: Vec<String>,          // skip_serializing_if empty
    pub annotations: Vec<String>,         // skip_serializing_if empty
    pub docstring_text: Option<String>,   // skip_serializing_if None
}
```

Fields removed vs `Signature`:

| Removed field | Reason |
|---|---|
| `has_docstring` | Claude does not need to document dep symbols |
| `line` | Claude will not navigate to lines in dep files |

`docstring_text` is kept: when a dep symbol is already documented, Claude can read that text instead of opening the dep file, saving tokens.

#### 3b. Refs-based filtering in `build_deps`

`build_deps` calls `compute_refs` (already in `src/commands/get.rs`) to determine which symbols the current file actually uses from each dep. Filtering rules per dep:

- **Top-level type declarations** (`kind` ∈ `{class, interface, enum, struct, trait, type_alias}`): always included as structural anchors.
- **Other signatures**: included only if `name` appears in the refs result for that dep.
- **Fallback** (refs returns empty for a dep — unsupported language, no symbols found, or source unreadable): include all signatures for that dep. No silent data loss.

`build_deps` becomes fallible (`anyhow::Result<Vec<DepEntry>>`). On I/O error reading source for refs, log a warning and fall back to unfiltered signatures.

## File Changes

| File | Change |
|---|---|
| `src/commands/scan.rs` | `to_string_pretty` → `to_string` |
| `src/commands/get.rs` | `to_string_pretty` → `to_string` |
| `src/commands/rescan.rs` | `to_string_pretty` → `to_string` (if applicable) |
| `src/commands/pom.rs` | `to_string_pretty` → `to_string` |
| `src/commands/next.rs` | Add `NextFileEntry`, `DepSignature`; update `NextOutput`; update `build_deps` |
| `src/models.rs` | No changes |

## Non-Goals

- No changes to `get`, `scan`, `pom`, `rescan` output structure — only serialization format.
- No changes to cache format or session format.
- No new CLI flags.
