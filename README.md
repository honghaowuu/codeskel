# codeskel

A fast Rust CLI that prepares a codebase for AI-driven code intelligence. It scans a project, detects languages, builds a dependency graph using **tree-sitter** for accurate parsing, topologically sorts files, extracts signatures (with annotation values and docstring text), and reports docstring coverage — all without consuming LLM tokens.

Results are cached to `.codeskel/cache.json`. The LLM queries individual file details on demand via `codeskel get`, keeping context usage constant regardless of project size.

## Installation

```bash
cargo install codeskel
```

## Usage

### 1. Scan a project

```bash
codeskel scan /path/to/project
```

Prints a small summary JSON to stdout and writes the full cache to `.codeskel/cache.json`:

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

`stats.to_comment` is the loop bound the LLM uses — files already well-commented or auto-generated are excluded.

**Options:**

```
-l, --lang <LANG>           Force language (java|python|ts|js|go|rust|cs|cpp|ruby)
    --include <GLOB>        Only include files matching glob (repeatable)
    --exclude <GLOB>        Exclude files matching glob (repeatable)
    --min-coverage <0-1>    Skip files above this docstring coverage [default: 0.8]
    --cache-dir <DIR>       Where to write cache [default: <project_root>/.codeskel]
-v, --verbose               Print progress to stderr
```

**Default excludes:** `**/test/**`, `**/tests/**`, `**/*Test.java`, `**/node_modules/**`, `**/vendor/**`, `**/.git/**`, `**/target/**`, `**/build/**`, `**/dist/**`

### 2. Query a file

```bash
# By index (0-based position in dependency order)
codeskel get .codeskel/cache.json --index 0

# By relative path
codeskel get .codeskel/cache.json --path src/main/java/com/example/model/User.java

# Signatures of direct dependencies (LLM context before commenting a file)
codeskel get .codeskel/cache.json --deps src/main/java/com/example/model/User.java

# Count of files in the transitive dependency chain (deepest dep first)
codeskel get .codeskel/cache.json --chain src/main/java/com/example/service/UserService.java

# Fetch one dep by position in the chain (0-based)
codeskel get .codeskel/cache.json --chain src/main/java/com/example/service/UserService.java --index 0

# Symbol references from a file's body to its internal deps (Java only)
codeskel get .codeskel/cache.json --refs src/main/java/com/example/service/UserService.java
```

`--index` and `--path` return the full file entry:

```json
{
  "path": "src/main/java/com/example/model/User.java",
  "language": "java",
  "package": "com.example.model",
  "comment_coverage": 0.1,
  "skip": false,
  "cycle_warning": false,
  "internal_imports": ["src/main/java/com/example/base/Entity.java"],
  "signatures": [
    {
      "kind": "class",
      "name": "User",
      "modifiers": ["public"],
      "extends": "Entity",
      "annotations": [
        { "name": "Entity", "value": null },
        { "name": "Auditable", "value": null }
      ],
      "line": 12,
      "has_docstring": false
    },
    {
      "kind": "method",
      "name": "findByEmail",
      "modifiers": ["public", "static"],
      "params": [{ "name": "email", "type": "String" }],
      "return_type": "Optional<User>",
      "throws": ["DatabaseException"],
      "annotations": [
        { "name": "Get", "value": null },
        { "name": "RequestMapping", "value": "/users" }
      ],
      "line": 34,
      "has_docstring": false
    }
  ]
}
```

`annotations` uses `{name, value}` format — `value` is the unnamed argument for
annotations like `@RequestMapping("/users")`, or `null` for markers or named-only
arguments. `docstring_text` is present when `has_docstring` is true.

`--chain` returns the count of files in a file's transitive dependency chain (deepest dep first, topo order):

```json
{ "for": "src/main/java/com/example/service/UserService.java", "count": 3 }
```

`--chain --index <i>` returns the full `FileEntry` for the i-th dep in that chain — identical format to `--index`. Index out of range → exit code 1.

`--refs` performs static analysis on the file's body and returns, for each internal dep, the symbol names actually referenced there (Java only; other languages emit `{ "for": ..., "refs": {} }`):

```json
{
  "for": "src/main/java/com/example/service/UserService.java",
  "refs": {
    "src/main/java/com/example/model/User.java": ["User", "getEmail"],
    "src/main/java/com/example/repo/UserRepository.java": ["UserRepository", "findById", "save"]
  }
}
```

Keys are relative paths of internal dep files. Values are symbol names (class, method, field, constructor) that appear in the dep's cached signatures and are actually used in the analyzed file. External/stdlib references are silently ignored.

`--deps` returns signature summaries of all direct dependencies — what the LLM loads as context before commenting a file:

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

### 3. Rescan after commenting

After the LLM writes docstrings to a file, update the cache:

```bash
codeskel rescan .codeskel/cache.json src/main/java/com/example/model/User.java
```

This re-parses the file and updates its coverage and signatures in-place so downstream dependents have accurate context.

### 4. Extract Maven POM metadata

```bash
codeskel pom /path/to/project [--controller-path src/main/java/Controller.java]
```

Reads `pom.xml` directly — no cache needed. Outputs compact JSON:

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

- **`service_name`**: from `<artifactId>`
- **`version`**: `${prop}` references resolved from `<properties>`
- **`is_multi_module`**: `true` if root POM has `<modules>`. Use `--controller-path` to select the relevant sub-module
- **`internal_sdk_deps`**: dependencies where `artifactId` ends in `-api` or `-sdk` AND `groupId` starts with the root POM `groupId` prefix
- **`existing_skill_path`**: absolute path to `docs/skills/<service_name>/SKILL.md` if present

## How the `comment` skill uses codeskel

**Project mode** — comment all files in topo order:

```
1. codeskel scan <project>          → { cache, stats.to_comment = N }
2. For i = 0..N-1:
   a. codeskel get <cache> --index i         → file details (annotations, docstring_text)
   b. codeskel get <cache> --deps <path>     → dependency signatures
   c. Read full source file
   d. Generate docstrings (LLM)
   e. Write file back
   f. codeskel rescan <cache> <path>         → update cache
```

**Targeted single-file mode** — comment one file and its transitive dep chain, touching only the symbols actually referenced:

```
1. codeskel scan <project>
2. codeskel get <cache> --chain <target>     → { count: N }
   If N = 0: skip to step 5.
3. Build refs_map upfront (one --refs call per chain file + target):
   codeskel get <cache> --refs <file>        → accumulate refs_map per dep
4. For i = 0..N-1:
   a. codeskel get <cache> --chain <target> --index i  → dep entry
   b. codeskel get <cache> --deps <dep_path>           → dep's dep signatures
   c. Read dep source; comment only items where has_docstring: false
      AND name ∈ refs_map[dep_path]
   d. codeskel rescan <cache> <dep_path>
5. Comment target file in full (all undocumented items):
   codeskel get <cache> --deps <target>      → context
   Read + comment target; codeskel rescan <cache> <target>
```

Both modes keep LLM context constant — at most one file entry + its deps' signatures + source at any point, regardless of project or chain size.

## How the `generate-microservice-skill` uses codeskel

```
1. codeskel pom [project_root] [--controller-path <path>]
      → service_name, group_id, version, internal_sdk_deps, existing_skill_path
      (replaces raw pom.xml reads)

2. codeskel scan <project_root> --lang java --include <controller_path>
      → cache + stats (one-time; only the controller layer)

3. codeskel get <cache> --path <ControllerFile.java>
      → signatures[] with annotations[].value and docstring_text per method
      (replaces reading full Java source)

      From annotations[].value:   extract @RequestMapping path prefix
      From has_docstring:          run Javadoc quality gate
      From docstring_text:        extract descriptions without reading source
```

Token savings vs. raw file reads:
- `pom.xml` (200+ lines) → ~10-field JSON object
- Each Java controller (300–500 lines) → signatures-only JSON (no method bodies)
- `@RequestMapping` value → direct field; no regex over raw source

## Supported Languages

| Language   | Extensions                    | Imports resolved by               |
|------------|-------------------------------|-----------------------------------|
| Java       | `.java`                       | Package prefix (inferred)         |
| Python     | `.py`                         | Module path resolution            |
| TypeScript | `.ts`, `.tsx`                 | Relative paths only (`./`, `../`) |
| JavaScript | `.js`, `.jsx`, `.mjs`         | Relative paths + `require()`      |
| Go         | `.go`                         | Module prefix from `go.mod`       |
| Rust       | `.rs`                         | `crate::` and `super::` paths     |
| C#         | `.cs`                         | Namespace prefix                  |
| C/C++      | `.c`, `.cpp`, `.h`, `.hpp`    | Quoted `#include "..."` only      |
| Ruby       | `.rb`                         | `require_relative` only           |

## Exit Codes

| Code | Meaning                        |
|------|--------------------------------|
| `0`  | Success                        |
| `1`  | Fatal error                    |
| `2`  | Partial success (warnings)     |

## Performance

- Parallel file parsing via `rayon`
- Handles projects with 10,000+ files
- Cache written once; individual file queries are instant reads

## Development

```bash
cargo build
cargo test
cargo run -- scan /path/to/project
```
