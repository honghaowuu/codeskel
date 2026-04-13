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

Entry mode: identical format to `codeskel get --index` (the full `FileEntry` object). Always returns `Ok(false)` matching existing `run` convention.

Index out of range → exit code 1 with message.
Count of 0 → file has no internal dependencies.

### Implementation

**`GetArgs` change:** Add `chain: Option<String>` field. Existing `--index` is reused as modifier.

**Mode validation refactor:** Five flags exist: `index`, `path`, `deps`, `chain`, `refs`. Mutual exclusion rules:

| Flags present | Valid? |
|---|---|
| `--path` alone | Yes |
| `--deps` alone | Yes |
| `--chain` alone | Yes (count mode) |
| `--chain --index <i>` | Yes (entry mode) |
| `--refs` alone | Yes |
| `--index` alone (no `--chain`) | Yes (existing behavior) |
| Any two of `{--path, --deps, --chain, --refs}` | No — exit 1 |
| `--index` with anything other than `--chain` | No — exit 1 |
| None of the above | No — exit 1 |

**Dispatcher pseudocode** for the refactored `run()`:

```
if chain.is_some() {
    if let Some(i) = index { get_chain_entry(cache, chain, i) }
    else                   { get_chain_count(cache, chain) }
} else if refs.is_some() {
    get_refs(cache, refs)
} else {
    // existing logic: --deps, --path, --index (standalone)
    if index.is_some() && (path.is_some() || deps.is_some()) → error
    if deps.is_some()  { get_deps(cache, deps) }
    else               { get_entry(cache, index, path) }  // existing behavior
}
```

`--index` without `--chain` falls through to the existing entry-by-index path. `--index` combined with `--path` or `--deps` (without `--chain`) is rejected inside the existing branch.

**`get_chain_count(cache, file_path) → anyhow::Result<bool>`:**
- Look up `file_path` in cache (exit 1 if missing)
- BFS/DFS over `internal_imports` starting from `file_path`'s imports, accumulating a visited set. `file_path` itself is **not** added to the visited set — the chain contains only the transitive dependencies, not the file being queried.
- Return `{ "for": file_path, "count": visited.len() }`. Count of 0 means no internal deps.

**`get_chain_entry(cache, file_path, index) → anyhow::Result<bool>`:**
- Compute transitive closure (same BFS as above, excluding `file_path` itself)
- Filter `cache.order` (already topo-sorted, leaves/deepest deps first) to entries in the closure, preserving order
- Return the `FileEntry` at position `index`; exit 1 with message if out of range
- Returns `Ok(false)` on success

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

Keys are relative paths of internal dep files. Values are symbol names referenced from that dep. Only names present in the dep's `signatures` appear (across all `kind` values — class, method, field, constructor, etc.). External/stdlib refs are silently ignored. Unsupported languages emit `{ "for": ..., "refs": {} }` with a stderr note.

### Architecture

**New `src/refs/mod.rs`** — trait definition and language dispatch:

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
2. Build `import_map`: for each dep path in `entry.internal_imports`, look up the dep's `FileEntry` and derive `simple_name` as the filename stem — i.e., `Path::new(dep_path).file_stem().and_then(|s| s.to_str())`, stripping the `.java` extension. This does not use the `package` field (which may be `None`). Map `simple_name → dep_path`.
3. Build `dep_sigs`: for each dep path in `entry.internal_imports`, collect all `sig.name` strings from the dep's `signatures` regardless of `sig.kind` (includes class, method, field, constructor, interface, enum). Map `dep_path → Vec<String>`.
4. Call `get_refs_analyzer(&lang)` — if `None` (unsupported language), emit `{ "for": ..., "refs": {} }` to stdout and a note to stderr; return `Ok(false)`
5. Read source file from disk: `Path::new(&cache.project_root).join(file_path)`
6. Run analyzer, emit result; return `Ok(false)`

### Java `RefsAnalyzer` — AST Walk

Uses the same tree-sitter Java grammar already in the project.

**Phase 1 — Build local type map** (pre-pass over full tree):

Walk all `local_variable_declaration`, `field_declaration`, and `formal_parameter` nodes.

- For `local_variable_declaration` and `field_declaration`: the type is at `child_by_field_name("type")` — accept `type_identifier` directly, or unwrap `generic_type → type_identifier` child. The variable name is inside the `variable_declarator` child (iterate children for kind `"variable_declarator"`) via `child_by_field_name("name")` on that child (same two-level pattern as `handle_field` in `parsers/java.rs`).
- For `formal_parameter`: same — `child_by_field_name("type")` for type. Name: find the `variable_declarator_id` child, then call `child_by_field_name("name")` on it to get the identifier.

Result: `HashMap<String, String>` of `var_name → declared_type_simple_name`.

**Phase 2 — Collect candidate references** (second pass):

Skip any node whose ancestor is an `import_declaration` or `package_declaration` — these produce false-positive type name matches.

| AST node | Access method | Action |
|---|---|---|
| `type_identifier` (not under import/package) | node text | Add type name as candidate `(type_name, nil)` |
| `object_creation_expression` | `child_by_field_name("type")` → text | Add `(type_name, nil)` for constructor ref |
| `method_invocation` | `child_by_field_name("object")` | Resolve receiver text: if in type map → use declared type; if starts uppercase → use directly (static call). Add `(resolved_type, method_name)` where `method_name` = `child_by_field_name("name")` |
| `field_access` | `child_by_field_name("object")` | Same receiver resolution. Add `(resolved_type, field_name)` where `field_name` = `child_by_field_name("field")` |

Chained calls: tree-sitter represents the receiver of `repo.findById(id).orElseThrow()` as a nested `method_invocation` node at `child_by_field_name("object")` — its text is not a simple identifier, so type map lookup fails and it is silently discarded. Only the outermost real receiver is resolved, matching PRD intent.

**Phase 3 — Cross-reference against dep signatures**:

For each candidate `(type_name, member_name_or_nil)`:
1. If `type_name` in `import_map`: look up `dep_path = import_map[type_name]`
2. Check `dep_sigs[dep_path]` contains `member_name_or_nil` (or `type_name` itself when `member_name_or_nil` is nil)
3. If match: add the matched name to output `refs[dep_path]`
4. If no match: discard silently

Dedup per dep file: output arrays contain each name at most once.

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
| `src/refs/mod.rs` | New — `RefsAnalyzer` trait + `get_refs_analyzer` dispatch |
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
