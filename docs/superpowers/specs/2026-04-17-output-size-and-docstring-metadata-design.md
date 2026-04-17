# Design: Output Size Control and Docstring Word Count Metadata

**Date:** 2026-04-17  
**Status:** Approved

## Background

Two pain points surfaced from AI-agent usage of `codeskel next`:

1. Dep entries for constants-heavy files (e.g. `AppExCode.java` with 200 error-code fields) produce 40+ KB of JSON, overwhelming agent context windows.
2. `has_docstring: false` is ambiguous — it means both "no docstring at all" and "docstring exists but is too thin (below `--min-docstring-words` threshold)". Agents must read the source file to distinguish these cases.

## Change 1: Kind-Based Field Truncation in `next` Deps

### Motivation

Fields on constants classes are low-value commentary targets. The refs analysis already filters dep signatures to referenced symbols, but a constants class may have 200 fields all technically referenced. The per-dep signature list needs a secondary budget for fields specifically.

### Design

Add `--max-fields N` (default `5`) to `NextArgs` in `src/cli.rs`.

In `build_deps` (`src/commands/next.rs`), after collecting filtered signatures per dep entry, apply field truncation:

1. Split signatures into non-fields (kind is not `"field"`) and fields (kind is `"field"`)
2. Keep all non-fields
3. Keep the first `max_fields` fields; discard the rest
4. If any fields were discarded, record the count

Add `fields_omitted: usize` to `DepEntry` (annotated with `skip_serializing_if = "is_zero"`). The dep entry itself always remains in the output — only field entries are truncated, never methods or type declarations.

**Example output** for a dep with 200 fields (max-fields=5):
```json
{
  "path": "src/AppExCode.java",
  "fields_omitted": 195,
  "signatures": [
    {"kind": "class", "name": "AppExCode", ...},
    {"kind": "field", "name": "USER_NOT_FOUND", ...},
    ...4 more fields...
  ]
}
```

A service interface with 12 methods and 0 fields is unchanged.

### Implementation Notes

- Add a free function `fn is_zero(n: &usize) -> bool { *n == 0 }` in both `src/commands/next.rs` (for `fields_omitted`) and `src/models.rs` (for `existing_word_count`), since they are separate modules.
- `max_fields` must be threaded through: `NextArgs` → `run_and_capture` → `run_project(cache_path, max_fields)` and `run_targeted(cache_path, target, max_fields)` → `build_deps(cache, file_entry, max_fields)`. All three private function signatures change.
- The effective output per dep entry is `non_field_signatures + min(field_signatures, max_fields)`. Non-field signatures (methods, constructors, type declarations) are never truncated.

### Files Affected

- `src/cli.rs` — add `max_fields: usize` to `NextArgs`
- `src/commands/next.rs` — add `fields_omitted` to `DepEntry` with `skip_serializing_if = "is_zero"`; add `is_zero` helper; update `run_and_capture`, `run_project`, `run_targeted`, `build_deps` signatures; add truncation logic in `build_deps`

---

## Change 2: `existing_word_count` on `Signature`

### Motivation

When `--min-docstring-words 30` is used during scan, `has_docstring: false` can mean:
- The file has no docstring at all → agent should write from scratch
- The file has a thin docstring (e.g. 10 words) → agent should improve existing

Without a word count, the agent must read the source file to determine which case applies. Adding `existing_word_count` surfaces this directly in the `next` output.

### Design

Add `existing_word_count: usize` to `Signature` in `src/models.rs`:
- `0` if no docstring text is present
- `N` (the actual word count) if `docstring_text` is present, regardless of whether it meets the `min_docstring_words` threshold

The value is computed in `apply_min_docstring_words` (`src/scanner.rs`), which already calls `count_prose_words` on each signature's `docstring_text`. Store the result on the signature at that point.

The current loop in `apply_min_docstring_words` is gated on `min_words > 0`. This must be restructured: the word-count population pass runs **unconditionally** over all signatures, while the `has_docstring = false` downgrade remains conditional on `min_words > 0`. This ensures `existing_word_count` is always populated, even in presence-only mode (`min_docstring_words = 0`).

`existing_word_count` is serialized with `skip_serializing_if = "is_zero"` — omitted from output when zero, present otherwise. Absence in the JSON implies zero (no docstring text).

**Interpretation by agent:**

| `has_docstring` | `existing_word_count` | Meaning |
|---|---|---|
| `false` | `0` | No docstring — write from scratch |
| `false` | `10` | Thin docstring (below threshold) — improve existing |
| `true` | `35` | Adequate docstring — may skip |

### Files Affected

- `src/models.rs` — add `existing_word_count: usize` to `Signature` with `skip_serializing_if = "is_zero"`
- `src/scanner.rs` — restructure `apply_min_docstring_words` to unconditionally populate `existing_word_count`; keep `has_docstring` downgrade conditional on `min_words > 0`
- `src/commands/next.rs` — add `existing_word_count` to `DepSignature` and update its `From<&Signature>` impl (agents read dep context from `next` output, so the field must appear there too)

---

## What We Are Not Implementing

- **`--max-depth N`**: Removing files from the dep chain creates silent comment gaps. The refs analysis already handles relevance filtering. Field truncation (Change 1) addresses the token budget problem more cleanly.
- **Session isolation (`--session <path>`)**: Already solved — `session.json` is separate from `cache.json`.
- **Target-scoped `scan`**: Scan happens once; effort not justified.
- **Comment skill improvements**: Left for a separate AI agent pass. See `docs/suggestions.md` for written responses.

---

## Testing

- **Change 1**: Two tests:
  - Default `max_fields = 5` with a dep file containing >5 fields: assert `fields_omitted` equals overflow count and signature list length equals `5 + non_field_count`.
  - `max_fields = 0`: assert all fields are omitted, `fields_omitted` equals total field count, non-field signatures intact.
- **Change 2**: Two tests:
  - Scan with `min_docstring_words = 30`, short docstring present: assert `existing_word_count > 0` and `has_docstring: false` (thin doc case).
  - Scan with `min_docstring_words = 0`, docstring present: assert `existing_word_count > 0` (word count populated even in presence-only mode).
