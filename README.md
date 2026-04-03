# codeskel

A fast Rust CLI that prepares a codebase for AI-driven comment generation. It scans a project, detects languages, builds a dependency graph using **tree-sitter** for accurate parsing, topologically sorts files, extracts signatures, and reports docstring coverage — all without consuming LLM tokens.

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
      "line": 12,
      "has_docstring": false
    },
    {
      "kind": "method",
      "name": "findByEmail",
      "modifiers": ["public", "static"],
      "params": [{"name": "email", "type": "String"}],
      "return_type": "Optional<User>",
      "line": 34,
      "has_docstring": false
    }
  ]
}
```

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

## How the `comment` skill uses codeskel

```
1. codeskel scan <project>          → { cache, stats.to_comment = N }
2. For i = 0..N-1:
   a. codeskel get <cache> --index i         → file details
   b. codeskel get <cache> --deps <path>     → dependency signatures
   c. Read full source file
   d. Generate docstrings (LLM)
   e. Write file back
   f. codeskel rescan <cache> <path>         → update cache
```

The LLM context at any step contains: one summary + one file's details + its deps' signatures + the source. No large arrays, constant memory regardless of project size.

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
