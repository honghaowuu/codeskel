# PRD: `codeskel get --refs` — Multi-Language Extension

## Overview

Extends `--refs` symbol reference extraction (currently Java-only) to Python, TypeScript/JavaScript, Go, and Rust. Each language uses the same three-phase approach already established for Java: build a local type/binding map, collect candidate references via tree-sitter AST walk, cross-reference against cached dep signatures.

The `RefsAnalyzer` trait and dispatch in `src/refs/mod.rs` are already wired for extension — only new `src/refs/<lang>.rs` files and dispatch arms are required.

---

## Current State

| Language | `--refs` support |
|---|---|
| Java | Full (phase 1–3, type map + method/field resolution) |
| Python | Emits `{ "for": ..., "refs": {} }` + stderr note |
| TypeScript | Emits `{ "for": ..., "refs": {} }` + stderr note |
| JavaScript | Emits `{ "for": ..., "refs": {} }` + stderr note |
| Go | Emits `{ "for": ..., "refs": {} }` + stderr note |
| Rust | Emits `{ "for": ..., "refs": {} }` + stderr note |

---

## Python

### Import map construction

The existing `import_map` is built from `entry.internal_imports` using the dep file's stem (e.g. `user.py` → `User` is wrong; Python stems are lowercase). For Python, the `import_map` key should be the **exact identifier** imported, not the stem.

Implementation: in `get_refs()`, when the file language is Python, derive `import_map` by reading `entry.internal_imports` and looking up each dep's `FileEntry`. Use the dep's `signatures[].name` where `kind == "class"` or `kind == "function"` as the simple names that map to that dep path.

### AST walk (three phases)

**Phase 1 — binding map:**

Walk `typed_parameter` (annotated function params), `assignment` with type annotations (`x: Type = ...`), and `expression_statement → assignment` nodes.

| Node | Binding |
|---|---|
| `typed_parameter` | `name → annotation type text` |
| `assignment` with `type` field (`x: Type = v`) | `name → type text` |
| `assignment` without type (plain `x = SomeClass()`) | record tentative binding to constructor call type (best-effort) |

For type text: strip brackets (`List[User]` → `User`) by extracting the first `type_identifier` child of a subscript.

**Phase 2 — candidate collection:**

| Node | Action |
|---|---|
| `import_from_statement` | Skip (handled via `import_map`) |
| `call` with `func = attribute` | Resolve `attribute.object` text → type map lookup; emit `(resolved_type, attribute.attribute text)` |
| `call` with `func = identifier` (uppercase) | Emit `(identifier, None)` (constructor call) |
| `attribute` outside a `call` | Resolve object; emit `(resolved_type, attribute text)` (field/property access) |
| `type` annotations in function signatures | Emit `(type_name, None)` for each `type_identifier` found |
| `isinstance(x, Type)` | Emit `(Type, None)` |

**Phase 3** — same cross-reference logic as Java.

### Edge cases

- `from .models import User, Repo` — both `User` and `Repo` are in `import_map` (pre-built from dep signatures)
- Chained calls (`self.repo.find(id).first()`) — resolve only `self.repo` if it's in the binding map; discard intermediate chained types
- `self` parameter — skip; never enters the type map
- Duck typing: untyped variables not in the binding map → silently discard, consistent with Java behavior

---

## TypeScript / JavaScript

### Import map construction

For TypeScript/JavaScript, `import_map` must be built from named and default import identifiers, not from file stems. Read `entry.internal_imports` (which stores dep file paths); for each dep path, look up the dep's `FileEntry` and map each `sig.name` where `kind ∈ {class, function, interface, type_alias}` to that dep path.

Named exports from a dep file → all their names in `import_map`. The import statement in the source file filters which names are actually used.

### AST walk (three phases)

**Phase 1 — binding map:**

| Node | Binding |
|---|---|
| `variable_declarator` with type annotation (`const x: UserService = ...`) | `x → UserService` |
| `variable_declarator` without annotation but with `new_expression` RHS | `x → new_expression.constructor text` (best-effort) |
| `required_parameter` / `optional_parameter` with type | `param_name → type text` |

For type text: strip generic arguments (`UserService<T>` → `UserService`) by taking the first `type_identifier` under `generic_type`.

**Phase 2 — candidate collection:**

| Node | Action |
|---|---|
| `import_statement` / `import_clause` | Skip |
| `call_expression` with `function = member_expression` | Resolve `member_expression.object` → emit `(resolved_type, member_expression.property text)` |
| `new_expression` | Emit `(constructor text, None)` |
| `member_expression` outside `call_expression` | Resolve object → emit `(resolved_type, property text)` (field/property access) |
| `type_identifier` in type annotations (`:` positions) | Emit `(type_name, None)` |
| `type_annotation → type_identifier` (e.g. `: UserService`) | Emit `(type_name, None)` |

**Phase 3** — same.

### JavaScript specifics

JavaScript has no type annotations. Phase 1 binding map is built entirely from `new_expression` RHS assignments. Phase 2 skips type annotation nodes. Coverage is necessarily lower than TypeScript — this is expected and consistent with the language's lack of static types.

### Edge cases

- Default imports (`import Foo from './foo'`) — `Foo` maps to the dep's default export; look for `sig.kind == "class"` or the first signature in the dep
- Namespace imports (`import * as Utils from './utils'`) — `Utils.methodName` → resolve as `(Utils-dep-path, methodName)`; emit all matching method names
- Re-exports: not resolved (out of scope — codeskel doesn't traverse dep deps for this purpose)

---

## Go

### Import map construction

Go imports use package paths, not simple names. The `import_map` key is the **package alias** used in the source file (last segment of the import path, or explicit alias). Build from `entry.internal_imports` using the dep's file stem as a proxy for the package name (or the explicit alias if parseable from the import declaration).

### AST walk (three phases)

**Phase 1 — binding map:**

| Node | Binding |
|---|---|
| `short_var_declaration` (`:=`) where RHS is `call_expression` on a selector | `lhs_name → selector.package` (best-effort for constructor calls like `models.NewUser()`) |
| `var_declaration` with explicit type | `var_name → type text` |
| `parameter_declaration` | `param_name → type text` (strip `*` pointer prefix) |

**Phase 2 — candidate collection:**

| Node | Action |
|---|---|
| `import_declaration` | Skip |
| `selector_expression` | Left side = package/var identifier; emit `(resolved_package_or_type, field/method text)` |
| `call_expression` with `function = selector_expression` | As above |
| `type_identifier` in composite literals, type assertions, conversions | Emit `(type_name, None)` |

For `selector_expression`: if the left side is a known package alias (in `import_map` as a package key), emit `(dep_path, right-side text)` directly. If the left side is a variable in the binding map, emit `(resolved_type, right-side text)`.

**Phase 3** — same.

### Edge cases

- Pointer receivers (`*UserService`) → strip `*` before map lookup
- Interface embedding: type assertions (`x.(UserRepository)`) → emit `(UserRepository, None)`
- Multiple return values from constructors (`u, err := models.NewUser()`) — only `u` is bound to the constructor type; `err` is always `error` (stdlib, discarded)

---

## Rust

### Import map construction

Rust `use` statements bring names into scope with their final path segment. Build `import_map` from `entry.internal_imports` using dep sig names (`sig.name` for all kinds).

### AST walk (three phases)

**Phase 1 — binding map:**

Rust has full type inference; explicit type annotations are less common. Collect what's available:

| Node | Binding |
|---|---|
| `let_declaration` with explicit type (`let x: UserService = ...`) | `x → UserService` |
| `parameter` with type | `param_name → type text` (strip `&`, `&mut`, lifetime params) |
| `let_declaration` with `call_expression` RHS on a known type (e.g. `UserService::new()`) | `x → UserService` (best-effort) |

**Phase 2 — candidate collection:**

| Node | Action |
|---|---|
| `use_declaration` | Skip |
| `field_expression` (`x.field`) | Resolve `x` → emit `(resolved_type, field text)` |
| `method_call_expression` (`x.method(...)`) | Resolve `x` → emit `(resolved_type, method text)` |
| `call_expression` with `function = scoped_identifier` (`TypeName::method(...)`) | Emit `(TypeName, method text)` and `(TypeName, None)` |
| `struct_expression` (`TypeName { ... }`) | Emit `(TypeName, None)` |
| `type_identifier` in type positions | Emit `(type_name, None)` |

For method calls without a resolvable receiver type: emit the method name against all dep sigs (may produce false positives if two deps export the same method name — acceptable for the use case).

**Phase 3** — same.

### Edge cases

- `impl Trait for Type` blocks: method references inside resolve against the implementing type
- `use crate::models::User` → `User` in scope; `use crate::models::*` (glob) → emit all dep sig names as candidates (conservative)
- Lifetimes in type positions (`&'a UserService`) → strip lifetime annotation before lookup

---

## Shared Architecture Changes

### `src/refs/mod.rs`

Add dispatch arms in `get_refs_analyzer`:

```rust
Language::Python     => Some(Box::new(python::PythonRefsAnalyzer::new())),
Language::TypeScript | Language::JavaScript => Some(Box::new(ts::TsRefsAnalyzer::new())),
Language::Go         => Some(Box::new(go::GoRefsAnalyzer::new())),
Language::Rust       => Some(Box::new(rust::RustRefsAnalyzer::new())),
```

### `src/commands/get.rs`

`get_refs()` already handles unsupported languages with `{ "for": ..., "refs": {} }` — no changes needed once the analyzers are wired in `get_refs_analyzer`.

For Python/TS/JS, the `import_map` construction logic (currently using file stem) needs a language-specific variant. Introduce a helper:

```rust
fn build_import_map(
    lang: &Language,
    internal_imports: &[String],
    cache: &Cache,
) -> HashMap<String, String>
```

- Java: current stem-based logic (`Path::file_stem()`)
- Python/TS/JS/Go/Rust: sig-name–based logic (collect all `sig.name` values from dep's `FileEntry.signatures`)

---

## File Map

| File | Change |
|---|---|
| `src/refs/python.rs` | New — `PythonRefsAnalyzer` |
| `src/refs/ts.rs` | New — `TsRefsAnalyzer` (TypeScript + JavaScript) |
| `src/refs/go.rs` | New — `GoRefsAnalyzer` |
| `src/refs/rust.rs` | New — `RustRefsAnalyzer` |
| `src/refs/mod.rs` | Add dispatch arms; add language-specific `build_import_map` variants |
| `src/commands/get.rs` | Pass language to `build_import_map` |
| `src/lib.rs` | No change needed |

---

## Non-Goals

- Full type inference or dataflow analysis for any language
- Resolving dynamic dispatch, reflection, or runtime-generated names
- Glob imports for Python (`from .models import *`) — emit no candidates from that import (conservative)
- C#, C/C++, Ruby — not in scope for this iteration
- Cross-file type resolution beyond the local binding map in the analyzed file

---

## Performance Requirements

All languages: under 500ms for a single file (one tree-sitter parse, in-memory cross-reference).

---

## Suggested Implementation Order

1. **TypeScript** — grammar already in the project (`tree-sitter-typescript`); named imports make the import map straightforward; type annotations give Phase 1 good coverage
2. **Python** — grammar already in the project (`tree-sitter-python`); annotated params and type hints give reasonable Phase 1 coverage
3. **Go** — grammar already in the project (`tree-sitter-go`); package-qualified selectors make Phase 2 unambiguous
4. **Rust** — grammar already in the project (`tree-sitter-rust`); Phase 1 is thin but Phase 2 struct/method patterns are clear

Each language is an independent `src/refs/<lang>.rs` file — implement and ship one at a time.
