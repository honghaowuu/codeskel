# Design: `codeskel get --chain` and `--refs`

**Date:** 2026-04-13
**Status:** Approved
**Scope:** Two new flags on `codeskel get` — transitive dep chain navigation and Java symbol reference extraction

---

## Context

The `comment` skill needs to operate in targeted single-file mode: comment a target file's entire transitive dependency chain, touching only the symbols actually used in each dep. Two gaps exist in the current `codeskel get` API:

1. No token-efficient way to iterate a file's transitive dep chain — the skill would have to load all dep paths upfront or re-implement graph traversal.
2. `--deps` returns all signatures of direct deps with no indication of which are actually referenced — the skill must either comment entire dep files or guess.

`--chain` and `--refs` close both gaps.

---

## Feature 1: `--chain`

### CLI

```
codeskel get <cache_path> --chain <file_path>
codeskel get <cache_path> --chain <file_path> --index <i>
```

`--chain` without `--index` returns the count of files in the transitive dep chain (deepest dep first, topo order). Token cost is constant regardless of chain depth.

`--chain --index <i>` returns the full `FileEntry` JSON for the i-th dep in that chain.

### Output

Count mode:
```json
{ "for": "src/.../UserService.java", "count": 3 }
```

Entry mode: identical format to `codeskel get --index` (the full `FileEntry` object).

Index out of range → exit code 1 with message.
Count of 0 → file has no internal dependencies.

### Implementation

**`GetArgs` change:** Add `chain: Option<String>` field. Existing `--index` is reused as modifier.

**Mode validation refactor:** Current `mode_count` logic treats `--index` as a standalone mode. New logic: modes are `{path, deps, chain, index-standalone}`. `--chain` with or without `--index` counts as one mode. `--index` alone (without `--chain`) remains its existing behavior.

**`get_chain_count(cache, file_path) → anyhow::Result<bool>`:**
- Look up `file_path` in cache (exit 1 if missing)
- BFS/DFS over `internal_imports` in cache entries, accumulating a visited set
- Return `{ "for": file_path, "count": visited.len() }`

**`get_chain_entry(cache, file_path, index) → anyhow::Result<bool>`:**
- Compute transitive closure (same BFS as above)
- Filter `cache.order` (already topo-sorted, deepest dep first) to entries in the closure
- Return the `FileEntry` at position `index`; error if out of range

Topo ordering reuses `cache.order` — O(N) in cache size, no re-sort needed. Well within 100ms / 50ms budget.

---

## Feature 2: `--refs`

### CLI

```
codeskel get <cache_path> --refs <file_path>
```

### Output

```json
{
  "for": "src/main/java/com/example/service/UserService.java",
  "refs": {
    "src/main/java/com/example/model/User.java": ["User", "findByEmail", "getEmail"],
    "src/main/java/com/example/repository/UserRepository.java": ["UserRepository", "save", "findById"]
  }
}
```

Keys are relative paths of internal dep files. Values are symbol names referenced from that dep. Only names present in the dep's `signatures` appear. External/stdlib refs are silently ignored. Unsupported languages emit `{ "for": ..., "refs": {} }` with a stderr note.

### Architecture

**New `src/refs.rs`** — trait definition and language dispatch:

```rust
pub trait RefsAnalyzer: Send + Sync {
    fn extract_refs(
        &self,
        source: &str,
        import_map: &HashMap<String, String>,    // simple_name → dep_file_path
        dep_sigs: &HashMap<String, Vec<String>>, // dep_file_path → [symbol names]
    ) -> HashMap<String, Vec<String>>;           // dep_file_path → [referenced names]
}

pub fn get_refs_analyzer(lang: &Language) -> Option<Box<dyn RefsAnalyzer>> { ... }
```

**New `src/refs/java.rs`** — Java implementation.

**Command flow in `get --refs`:**
1. Load cache, look up file entry (exit 1 if missing)
2. Build `import_map`: for each dep path in `entry.internal_imports`, derive simple name from dep entry's `package` + last path segment of filename (strip `.java`)
3. Build `dep_sigs`: for each dep, collect all `sig.name` strings
4. Call `get_refs_analyzer(&lang)` — `None` → emit empty refs + stderr note
5. Read source file from disk (using `cache.project_root` + relative path)
6. Run analyzer, emit result

### Java `RefsAnalyzer` — AST Walk

Uses the same tree-sitter Java grammar already in the project.

**Phase 1 — Build local type map** (pre-pass):

Walk all `local_variable_declaration`, `field_declaration`, and `formal_parameter` nodes. For each, extract `type_identifier` (unwrap `generic_type` → `type_identifier` if needed) as the declared type, and the variable/parameter name. Result: `HashMap<String, String>` of `var_name → declared_type`.

**Phase 2 — Collect candidate references**:

| AST node | Action |
|---|---|
| `type_identifier` (in type position) | Add type name as candidate |
| `object_creation_expression` → type child | Add constructor type name |
| `method_invocation` with `object` child | Resolve receiver: if identifier in type map → use declared type; if capitalized → use directly (static call). Add `(resolved_type, method_name)` |
| `field_access` with `object` child | Same receiver resolution. Add `(resolved_type, field_name)` |

Chained calls: tree-sitter represents the receiver of `repo.findById(id).orElseThrow()` as a nested `method_invocation` node, not an identifier — resolution fails and it is silently discarded. Only the outermost real receiver is resolved, matching PRD intent.

**Phase 3 — Cross-reference**:

For each candidate `(type_name, member_name_or_nil)`:
- If `type_name` in `import_map`: look up `dep_sigs[dep_path]`, check membership of `member_name_or_nil` (or `type_name` for type-only refs) → add to output
- Otherwise discard

**Edge cases (per PRD):**
- Overloaded methods: matched by name only; all overloads are candidates
- Same symbol name in two deps: added to both
- File not in cache: exit 1
- No internal imports: `{ "for": ..., "refs": {} }`
- Dep has no signatures: key still appears with whatever names matched

---

## File Map

| File | Change |
|---|---|
| `src/cli.rs` | Add `chain: Option<String>` to `GetArgs`; add `refs: Option<String>` to `GetArgs` |
| `src/commands/get.rs` | Refactor mode validation; add `get_chain_count`, `get_chain_entry`, `get_refs` functions |
| `src/refs.rs` | New — `RefsAnalyzer` trait + `get_refs_analyzer` dispatch |
| `src/refs/java.rs` | New — `JavaRefsAnalyzer` tree-sitter walk |
| `src/lib.rs` | Add `pub mod refs` |

No changes to `scan`, `rescan`, `models`, `parsers`, or `cache`.

---

## Non-Goals

- Full type inference (no dataflow, no cross-file type resolution beyond declared types)
- Languages other than Java in this iteration
- Runtime reference tracking (dynamic dispatch, reflection)
- Detecting unused imports

---

## Performance

- `--chain` count: under 100ms (in-memory BFS)
- `--chain --index`: under 50ms (single cache lookup after BFS)
- `--refs` (Java): under 500ms (single tree-sitter parse)
- Memory for `--refs`: proportional to analyzed file size only
