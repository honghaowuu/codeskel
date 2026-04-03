# PRD: `codeskel` — Project Dependency & Coverage Scanner

## Overview

`codeskel` is a fast Rust CLI tool that prepares a codebase for AI-driven code intelligence tasks. It scans a project, detects languages, builds a dependency graph using **tree-sitter** for accurate parsing, topologically sorts files, extracts signatures (including docstring text and annotation values), and reports docstring coverage — all without consuming LLM tokens.

Results are cached to `.codeskel/cache.json` in the project root. The LLM queries individual file details on demand via `codeskel get`, keeping context usage minimal regardless of project size.

Primary consumers:
- **`comment` skill** — drives iterative Javadoc/docstring generation across a project
- **`generate-microservice-skill`** — replaces raw file reads with structured metadata for Spring controller analysis and Maven POM extraction

---

## CLI Interface

### `codeskel scan` — analyze project (run once)

```
codeskel scan [OPTIONS] <project_root>

Arguments:
  <project_root>    Path to the project root

Options:
  -l, --lang <lang>         Force language (java|python|ts|js|go|rust|cs|cpp|ruby)
                            If omitted, auto-detect per file by extension
  --include <glob>          Only include files matching glob (repeatable)
  --exclude <glob>          Exclude files matching glob (repeatable)
                            Default excludes: **/test/**, **/tests/**, **/*Test.java,
                            **/node_modules/**, **/vendor/**, **/.git/**,
                            **/target/**, **/build/**, **/dist/**
  --min-coverage <0.0-1.0>  Skip files with existing comment coverage above this
                            threshold [default: 0.8]
  --cache-dir <dir>         Where to write cache [default: <project_root>/.codeskel]
  -v, --verbose             Print progress to stderr
  -h, --help                Print help
```

Prints a **summary JSON** to stdout (small, safe to load into LLM context). Saves full data to `<cache-dir>/cache.json`.

**Summary output:**
```json
{
  "project_root": "/abs/path/to/project",
  "detected_languages": ["java", "python"],
  "cache": "/abs/path/to/project/.codeskel/cache.json",
  "stats": {
    "total_files": 142,
    "skipped_covered": 18,
    "skipped_generated": 5,
    "to_comment": 119
  }
}
```

The LLM only needs `stats.to_comment` to know the loop bound. All file details are fetched by index on demand — no large arrays are ever held in context.

---

### `codeskel get` — query individual file (called per file during commenting)

```
codeskel get <cache_path> [--index <N> | --path <file_path>]
codeskel get <cache_path> --deps <file_path>

Arguments:
  <cache_path>      Path to .codeskel/cache.json

Options:
  --index <N>       Return file at position N in the order array (0-based)
  --path <path>     Return file by path (relative to project root)
  --deps <path>     Return signature summaries of direct dependencies of this file
                    (the already-documented context the LLM needs before commenting)
```

**`--index` / `--path` output** (full file entry):
```json
{
  "path": "src/main/java/com/example/model/User.java",
  "language": "java",
  "package": "com.example.model",
  "comment_coverage": 0.1,
  "skip": false,
  "skip_reason": null,
  "cycle_warning": false,
  "internal_imports": [
    "com.example.base.Entity",
    "com.example.base.Auditable"
  ],
  "signatures": [
    {
      "kind": "class",
      "name": "SubscriptionController",
      "modifiers": ["public"],
      "annotations": [
        {"name": "RestController", "value": null},
        {"name": "RequestMapping", "value": "/api/v1/subscriptions"}
      ],
      "line": 10,
      "has_docstring": true,
      "docstring_text": "Manages subscription lifecycle for billing."
    },
    {
      "kind": "method",
      "name": "createSubscription",
      "modifiers": ["public"],
      "params": [{"name": "req", "type": "CreateSubscriptionRequest"}],
      "return_type": "SubscriptionResponse",
      "annotations": [
        {"name": "PostMapping", "value": null}
      ],
      "line": 22,
      "has_docstring": true,
      "docstring_text": "Creates a new subscription for a user. Requires the user to exist in user-service. Pass a unique requestId for idempotency."
    },
    {
      "kind": "field",
      "name": "id",
      "type": "Long",
      "modifiers": ["private"],
      "annotations": [],
      "line": 15,
      "has_docstring": false,
      "docstring_text": null
    }
  ]
}
```

**`--deps` output** (signatures of all direct dependencies, compacted):
```json
{
  "for": "src/main/java/com/example/model/User.java",
  "dependencies": [
    {
      "path": "src/main/java/com/example/base/Entity.java",
      "signatures": [ ... ]
    }
  ]
}
```

This is what the LLM loads as context before commenting a file — the public API surface of everything that file imports, without loading full file bodies.

---

### `codeskel rescan` — re-analyze specific files (after commenting)

```
codeskel rescan <cache_path> <file_path> [<file_path> ...]
```

Re-parses listed files and updates their entries in the cache (coverage, signatures). Run after commenting a file to keep the cache fresh for downstream dependents.

---

### `codeskel pom` — extract Maven project metadata (no cache needed)

```
codeskel pom [OPTIONS] [project_root]

Arguments:
  [project_root]            Path to the project root (default: current directory)

Options:
  --controller-path <path>  Path hint for multi-module resolution: selects the
                            sub-module whose directory contains this path
  -h, --help                Print help
```

Reads `pom.xml` directly — no cache required. Prints JSON to stdout. Exits 1 to stderr if no `pom.xml` found.

**Output:**
```json
{
  "service_name": "billing-service",
  "group_id": "com.example",
  "version": "1.2.0",
  "pom_path": "/abs/path/to/billing-service/pom.xml",
  "is_multi_module": false,
  "internal_sdk_deps": [
    { "artifact_id": "user-api", "group_id": "com.example", "version": "2.1.0" }
  ],
  "existing_skill_path": null
}
```

**Rules:**

- **Multi-module Maven:** If root `pom.xml` has a `<modules>` block, walk into the sub-module whose directory path contains `--controller-path`. Use that sub-module's `pom.xml` for extraction. Inherit `groupId` from parent POM if absent in the module POM. Set `is_multi_module: true`.
- **Version resolution:** If `<version>` is a property reference (e.g. `${billing.version}`), resolve it from `<properties>` in the module POM first, then the parent POM. Output the resolved literal string.
- **`internal_sdk_deps`:** Filter `<dependency>` entries where `artifactId` ends in `-api` or `-sdk` AND `groupId` starts with the same prefix as the root POM `groupId` (e.g. root `com.example` → match `com.example.*`).
- **`existing_skill_path`:** Check whether `docs/skills/<service_name>/SKILL.md` exists under `project_root`. Return its absolute path if present, `null` otherwise.
- **Error cases:** No `pom.xml` found anywhere → exit 1. Missing `artifactId` or `groupId` in all reachable POMs → exit 1 with descriptive message.

---

## Parsing: tree-sitter

All parsing (imports, signatures, docstring detection) uses **tree-sitter** with the appropriate language grammar. This ensures accuracy with:
- Nested generics (`Map<String, List<Optional<T>>>`)
- Annotations (`@Override`, `@Deprecated`)
- Multi-line declarations
- String literals that happen to look like import statements
- Conditional imports (Python `if TYPE_CHECKING:` blocks)

**Tree-sitter grammars to bundle:**

| Language | Grammar crate |
|---|---|
| Java | `tree-sitter-java` |
| Python | `tree-sitter-python` |
| TypeScript | `tree-sitter-typescript` |
| JavaScript | `tree-sitter-javascript` |
| Go | `tree-sitter-go` |
| Rust | `tree-sitter-rust` |
| C# | `tree-sitter-c-sharp` |
| C/C++ | `tree-sitter-cpp` |
| Ruby | `tree-sitter-ruby` |

Parse errors are non-fatal: fall back to empty signatures for that node, log warning to stderr.

---

## Language Support

### Detection

Auto-detect by file extension. A project may contain multiple languages.

| Language | Extensions |
|---|---|
| Java | `.java` |
| Python | `.py` |
| TypeScript | `.ts`, `.tsx` |
| JavaScript | `.js`, `.jsx`, `.mjs` |
| Go | `.go` |
| Rust | `.rs` |
| C# | `.cs` |
| C/C++ | `.c`, `.cpp`, `.h`, `.hpp` |
| Ruby | `.rb` |

### Internal Import Resolution (per language)

Only imports that resolve to files within the project are treated as internal dependencies. tree-sitter extracts the import nodes; resolution logic maps them to file paths.

**Java**
- Extract `import_declaration` nodes
- Internal: package prefix matches the project's dominant package (inferred from most common prefix across all `.java` files)
- Skip: `java.*`, `javax.*`, `org.springframework.*`, etc.

**Python**
- Extract `import_statement` and `import_from_statement` nodes
- Internal: module path resolves to a file under project root or `src/`
- Skip: stdlib modules (checked against Python stdlib list), anything not resolvable locally

**TypeScript / JavaScript**
- Extract `import_statement` and `call_expression` (for `require()`) nodes
- Internal: only relative paths (`./`, `../`)
- Skip: bare specifiers (`express`, `path`, etc.)

**Go**
- Extract `import_spec` nodes
- Internal: import path matches module prefix in `go.mod`
- Skip: stdlib (single-component paths like `"fmt"`, `"os"`)

**Rust**
- Extract `use_declaration` nodes
- Internal: `crate::` or `super::` paths
- Skip: `std::`, `core::`, external crates (anything not in `src/`)

**C#**
- Extract `using_directive` nodes
- Internal: namespace prefix matches project's root namespace (inferred from `.csproj` or most common prefix)
- Skip: `System.*`, `Microsoft.*`

**C/C++**
- Extract `preproc_include` nodes
- Internal: quoted includes (`#include "..."`)
- Skip: angle-bracket includes (`#include <...>`)

**Ruby**
- Extract `call` nodes for `require_relative` (always internal) and `require`
- `require` is internal only if the path resolves to a file under project root

---

## Dependency Graph & Ordering

### Algorithm

1. Build directed graph: edge A → B means "A imports B"
2. Topological sort via Kahn's algorithm
3. Cycle detection: when a cycle is found, break it by removing the edge between the two nodes with the most shared imports (most tightly coupled → most context available without the edge). Log warning.

### Cycle Handling

- Add `"cycle_warning": true` to both files' cache entries
- The skill reads both files in full when either has this flag

---

## Comment Coverage Calculation

Coverage = (documentable items with docstrings) / (total documentable items)

**Documentable items**: classes, interfaces, enums, public/protected methods, constructors, public/protected fields.

Detected via tree-sitter: check whether the sibling node immediately preceding a declaration is a doc comment node.

| Language | Doc comment node type |
|---|---|
| Java | `block_comment` starting with `/**` |
| Python | `expression_statement` containing `string` as first body statement |
| TypeScript/JS | `comment` starting with `/**` |
| Go | `comment` immediately before declaration matching `// FuncName` |
| Rust | `line_comment` with `///` or `block_comment` with `//!` |
| C# | `comment` with `///` |
| C/C++ | `comment` starting with `/**` or `/*!` |
| Ruby | `comment` block immediately preceding class/method |

Files with coverage ≥ `--min-coverage` (default 0.8): `skip: true`, `skip_reason: "sufficient_coverage"`.

---

## Signature Extraction

Extracted via tree-sitter AST traversal. No method bodies — declarations only.

Fields captured per signature item:

| Field | Type | Description |
|---|---|---|
| `kind` | string | `class` \| `interface` \| `enum` \| `method` \| `constructor` \| `field` \| `function` \| `struct` \| `trait` \| `type_alias` |
| `name` | string | Identifier name |
| `modifiers` | string[] | e.g. `["public", "static", "final"]` |
| `params` | `{name, type}[]` | Method/constructor parameters |
| `return_type` | string \| null | Return type (null for constructors, void) |
| `throws` | string[] | Checked exceptions (Java, Python raises) |
| `extends` | string \| null | Superclass |
| `implements` | string[] | Interfaces implemented |
| `annotations` | `{name, value}[]` | Annotation name (without `@`) and its default (unnamed) argument value, or `null` if the annotation has no arguments or uses named-only arguments. e.g. `[{"name":"RequestMapping","value":"/api/v1/users"},{"name":"Override","value":null}]` |
| `line` | int | Line number in source file |
| `has_docstring` | bool | Whether a doc comment precedes this item |
| `docstring_text` | string \| null | Raw text of the preceding doc comment, stripped of delimiters (`/** */`, leading ` * `). `null` when `has_docstring` is false. Populated for all documentable items (classes, methods, constructors, public/protected fields). |

**Fields are included** in documentable items. Public/protected fields are extracted with their type — this is important for API/SDK projects where field semantics matter to callers.

---

## Generated File Detection

Skip files that appear auto-generated. Detected by:
- First 10 lines contain: `@Generated`, `// Code generated`, `DO NOT EDIT`, `Auto-generated`, `THIS FILE IS AUTOMATICALLY GENERATED`
- Path matches: `**/generated/**`, `**/*_pb.java`, `**/*.g.ts`, `**/migrations/**`, `**/*_gen.go`

Mark: `skip: true`, `skip_reason: "generated"`.

---

## Cache Format

`.codeskel/cache.json` — full analysis, not intended to be read in full by the LLM.

```json
{
  "version": 1,
  "scanned_at": "2026-04-02T10:00:00Z",
  "project_root": "/abs/path",
  "detected_languages": ["java"],
  "stats": { ... },
  "files": {
    "<relative_path>": { /* full file entry as shown in get output */ }
  }
}
```

`codeskel rescan` updates individual entries and bumps `scanned_at` for those entries only.

---

## Performance Requirements

- Handle projects with 10,000+ files
- `codeskel scan` completes in under 10 seconds for a 500-file Java project
- Memory: under 300MB for 10,000-file projects
- Parallelize file parsing with `rayon`
- tree-sitter parsers are reused across files (not re-initialized per file)

---

## Error Handling

| Condition | Behavior |
|---|---|
| Unreadable file | Warn stderr, skip file |
| tree-sitter parse error | Warn stderr, include file with `signatures: []`, `internal_imports: []` |
| Unsupported language | Warn stderr, include file with empty signatures |
| `go.mod` not found | Treat all Go imports as external |
| Cycle in dependency graph | Break cycle, set `cycle_warning: true`, warn stderr |
| Cache dir not writable | Exit code 1 |
| `get` index out of range | Exit code 1 with message |
| `get` path not found in cache | Exit code 1 with message |

Exit codes: `0` success, `1` fatal error, `2` partial success (warnings issued).

---

## Installation

```bash
cargo install codeskel
```

The `comment` skill checks for `codeskel` in PATH on startup. If not found:

```
codeskel is required but not installed.
Run: cargo install codeskel
Then retry.
```

---

## How the `comment` skill uses `codeskel`

```
1. codeskel scan <project>          → { cache, stats.to_comment = N }
2. For i = 0..N-1:
   a. codeskel get <cache> --index i         → file details (skip? cycle_warning?)
   b. codeskel get <cache> --deps <path>     → dep signatures (context for LLM)
   c. Read full source file
   d. Generate docstrings (LLM)
   e. Write file back
   f. codeskel rescan <cache> <path>         → update cache with new coverage
```

The LLM context at any point contains: the summary stats + one file's details + its deps' signatures + the file source. No large arrays, constant memory regardless of project size.

---

## How the `generate-microservice-skill` uses `codeskel`

```
1. codeskel pom [project_root] [--controller-path <path>]
      → service_name, group_id, version, internal_sdk_deps, existing_skill_path
      (replaces raw pom.xml read + manual XML parsing in Step 1)

2. codeskel scan <project_root> --lang java --include <controller_path>
      → { cache, stats }
      (one-time index; includes only the controller path, not the whole project)

3. codeskel get <cache> --path <ControllerFile.java>
      → signatures[] with annotations[].value and docstring_text per method
      (replaces reading the full Java source file in Step 2)

      From annotations[].value:   extract @RequestMapping path prefix
      From has_docstring:          run Javadoc quality gate
      From docstring_text:         extract business descriptions for capabilities,
                                   scenarios, and notes — without reading raw source
```

Token savings vs. baseline (reading raw files):
- `pom.xml` (often 200+ lines) → replaced by a ~10-field JSON object
- Each Java controller (often 300–500 lines) → replaced by signatures-only JSON (no method bodies, imports, or annotations in prose form)
- Javadoc quality check → `has_docstring` boolean; no re-read needed
- `@RequestMapping` value → direct field; no regex over raw source

The `generate-microservice-skill` only needs to invoke `codeskel scan` once per session. Subsequent `codeskel get` calls per controller file stay O(1) in context cost.

---

## Non-Goals

- Not a linter or formatter
- Does not modify source files (read-only, except writing cache)
- Does not resolve transitive dependencies (direct imports only)
- Does not parse method bodies or understand runtime behavior
- Does not generate comments itself
