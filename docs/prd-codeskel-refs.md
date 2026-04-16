# PRD: `codeskel get --chain` and `--refs` — Dep Chain Navigation & Symbol Reference Extraction

## Overview

Extends `codeskel get` with two new flags:

- **`--chain`** — returns the size of a file's transitive dependency chain (token-efficient, like `scan` stats), with **`--chain --index <i>`** to fetch one dep at a time in topo order
- **`--refs`** — performs static analysis on a source file's body and returns, for each internal dependency, the set of symbol names (classes, methods, fields, constructors) that are actually referenced in the file

Together these enable the `comment` skill to operate in **targeted single-file mode**: comment a target file's entire transitive dep chain in a token-efficient loop, touching only the items actually used.

---

## Motivation

**`--chain` / `--chain --index`:** The skill currently has no token-efficient way to iterate a file's transitive dep chain. Without this, the skill must either load all dep paths upfront (wasteful — large arrays in context) or re-implement graph traversal itself (fragile). `--chain` mirrors the pattern of `codeskel scan`: return only a count first, then fetch one entry at a time. The LLM context holds at most one file entry at a time regardless of chain depth.

**`--refs`:** The existing `codeskel get --deps <file>` returns the *signatures* of a file's direct dependencies but gives no indication of which of those signatures are actually used. Without this, targeted dep-aware commenting requires either:

- Commenting entire dependency files (wasteful, changes unintended code), or
- Having the LLM guess which symbols are referenced (imprecise, puts static analysis in the prompt)

`--refs` moves this analysis into codeskel where it belongs: fast, deterministic, no LLM tokens consumed.

---

## CLI Interface

### `--chain` — transitive dep chain size

```
codeskel get <cache_path> --chain <file_path>
```

Returns only the count of files in the transitive dependency chain (topo-sorted, deepest dep first). No file paths in the output — token cost is constant regardless of chain size.

```json
{
  "for": "src/main/java/com/example/service/UserService.java",
  "count": 3
}
```

If `count` is 0, the file has no internal dependencies — skip straight to commenting the target file.

### `--chain --index <i>` — fetch one dep by position

```
codeskel get <cache_path> --chain <file_path> --index <i>
```

Returns the full cache entry for the i-th file in the transitive dep chain (0-based, deepest dep first) — identical format to `codeskel get --index` in project mode.

```json
{
  "path": "src/main/java/com/example/model/User.java",
  "language": "java",
  "comment_coverage": 0.1,
  "cycle_warning": false,
  "internal_imports": ["com.example.base.Entity"],
  "signatures": [ ... ]
}
```

Index out of range → exit code 1 with message.

### `--refs` — symbol references from a file's body

```
codeskel get <cache_path> --refs <file_path>
```

| Argument | Description |
|---|---|
| `<cache_path>` | Path to `.codeskel/cache.json` |
| `--refs <file_path>` | Path of the file to analyze (relative to project root) |

```json
{
  "for": "src/main/java/com/example/service/UserService.java",
  "refs": {
    "src/main/java/com/example/model/User.java": ["User", "findByEmail", "getEmail"],
    "src/main/java/com/example/repository/UserRepository.java": ["UserRepository", "save", "findById"]
  }
}
```

- Keys are **relative paths** of internal dependency files (must already exist in the cache)
- Values are **arrays of symbol names** referenced from that dep file
- Only internal dependencies (those tracked in the cache) appear as keys
- External/stdlib references are silently ignored

---

## Reference Extraction Algorithm

### Step 1 — Load imported names

For each internal import in the file (already known from the cache entry), record the **simple name** of the imported entity:

- Java: `import com.example.model.User` → `User`
- Python: `from .models import User, UserRepository` → `User`, `UserRepository`
- TypeScript: `import { UserService } from './UserService'` → `UserService`
- Go: last path segment of import, or alias if present

Build a map: `simple_name → dep_file_path`.

### Step 2 — Extract referenced names from the file body

Using tree-sitter, traverse the file's AST and collect:

| Node type | What to extract |
|---|---|
| `type_identifier` / type annotations | Class/interface names used as types |
| `object_creation_expression` / `new` | Constructor calls (`new User()` → `User`) |
| `method_invocation` on a typed variable | Method name + receiver's declared type |
| `field_access` on a typed variable | Field name + receiver's declared type |
| Direct name references | Any identifier matching an imported simple name |

For **method and field access**, codeskel needs to resolve the receiver's declared type. This is done via a lightweight local type map: scan the file body for variable declarations (`User user = ...`, `UserRepository repo`) and build `variable_name → declared_type`. No full type inference — only direct declarations in the current file.

### Step 3 — Cross-reference with dep signatures

For each collected `(receiver_type, member_name)` or `(class_name)`:

1. Look up the dep file via `simple_name → dep_file_path`
2. Check that the name exists in that dep file's `signatures` (from cache)
3. If matched: add to the refs output for that dep file
4. If no match (e.g. stdlib type, or chained call result): discard silently

### Step 4 — Emit

Output the refs map. Symbol names with no match in any dep file's signatures are excluded.

---

## Per-Language Notes

### Java

- Variable declaration: `Type varName =` or `Type varName;` nodes
- Method call receiver: `method_invocation.object` → look up its declared type
- Chained calls (`repo.findById(id).orElseThrow()`): only the outermost receiver is resolved; chained method names on intermediate types (e.g. `Optional`) are discarded as they won't be in project deps
- Static method calls: `ClassName.methodName(...)` → `ClassName` resolves directly via import map

### Python

- Variable annotations: `var: Type =`
- `isinstance(x, Type)` → `Type`
- Direct attribute access: `obj.method()` → resolve `obj` via local type map
- `from .module import name` → `name` added directly to import map

### TypeScript / JavaScript

- Named imports: `import { Foo, bar } from './foo'` → `Foo`, `bar`
- Default imports: `import Foo from './foo'` → `Foo`
- Variable declarations with type annotations: `const svc: UserService = ...`
- Without type annotations, only named imports that appear as identifiers are tracked

### Go

- `:=` assignments with known return types (limited — only when RHS is a constructor-style call `pkg.NewFoo()`)
- Selector expressions: `pkg.FuncName` → resolve `pkg` via import alias map

### Rust

- `use` paths → extract the final segment as the simple name
- Struct instantiation: `TypeName { ... }` → `TypeName`
- Method calls: `value.method_name()` → without type inference, only track `method_name` against all dep signatures (may match across files; prefer the dep that exports it)

---

## Transitive Reference Propagation (used by the skill, not codeskel)

`--refs` returns direct references only (symbols used in the analyzed file's body). For transitive dep commenting, the skill is responsible for propagation:

When the skill needs to decide which items to comment in a deep dep file `C` (where chain is `A → B → C`):

1. `--refs A` → what A uses from B (and directly from C if A imports C)
2. `--refs B` → what B uses from C
3. Union of all refs pointing to C = symbols to comment in C

The skill accumulates a `refs_map: dep_file → Set<symbol_name>` as it walks the dep chain, then uses it when commenting each file.

---

## Edge Cases

| Case | Behavior |
|---|---|
| Overloaded methods (same name, different params) | All overloads are matched by name; all are candidates for commenting |
| Symbol name collision across two dep files | Both deps get the name added to their refs |
| File not in cache | Exit code 1 with message |
| File has no internal imports | Output `{ "for": "...", "refs": {} }` |
| Dep file has no signatures (e.g. parse failed) | Still appears as key with whatever names were matched; skill will find nothing to comment |
| Dynamic dispatch / reflection | Not resolved — out of scope for static analysis |

---

## Integration with `comment` skill

The single-file workflow mirrors project mode — one file in context at a time:

```
1. codeskel scan <project_root>
   → { cache, stats }                          (build dep graph)

2. codeskel get <cache> --chain <target_file>
   → { count: N }                              (how many deps to process)

   If N = 0: skip to step 5.

3. Build refs_map upfront (one --refs call per file in chain + target):
   For each file F that is in the chain or is the target:
     codeskel get <cache> --refs F
     → accumulate refs_map[dep_path] += symbol_names

4. For i = 0 to N-1:                          (mirrors project mode loop)
   a. codeskel get <cache> --chain <target_file> --index i
      → dep file entry (path, signatures, cycle_warning?)
   b. codeskel get <cache> --deps <dep_path>
      → dep's own dependency signatures (context for commenting)
   c. Read dep file source
   d. Comment only items where has_docstring: false
      AND name ∈ refs_map[dep_path]
   e. codeskel rescan <cache> <dep_path>
   Print: [dep i+1/N] <dep_path>

5. Comment target file in full (all undocumented items):
   codeskel get <cache> --deps <target_file>   (context)
   Read + comment target file source
   codeskel rescan <cache> <target_file>
```

**Token profile:** At any point the LLM context holds the chain count + one dep entry + its deps' signatures + the dep source. Constant memory regardless of chain depth — identical to project mode.

No changes to `codeskel scan`, `codeskel get --index/--path/--deps`, or `codeskel rescan`.

---

## Non-Goals

- Full type inference (no dataflow analysis, no cross-file type resolution beyond declared types)
- Runtime reference tracking (dynamic dispatch, reflection)
- Detecting unused imports
- Modifying or suggesting removal of any code

---

## Performance Requirements

- `codeskel get --chain <file>` completes in under 100ms (graph traversal over in-memory cache)
- `codeskel get --chain <file> --index <i>` completes in under 50ms (single cache lookup)
- `codeskel get --refs <file>` for a single file completes in under 500ms
- Memory usage for `--refs` proportional to the size of the analyzed file only (dep signatures already in cache)
- tree-sitter parser reused from existing scan (not re-initialized)
