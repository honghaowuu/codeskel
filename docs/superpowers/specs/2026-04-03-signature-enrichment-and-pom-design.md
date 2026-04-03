# Design: Signature Enrichment + `codeskel pom` Subcommand

**Date:** 2026-04-03  
**Status:** Approved  
**Scope:** Three features from the 2026-04-03 PRD changelog

---

## Context

The `generate-microservice-skill` needs structured metadata from Java source and Maven POMs
without loading raw file content into LLM context. Two gaps existed in the current
`codeskel get` output: docstring text was not extracted (only a boolean), and annotation
argument values were not captured (only annotation names). A new `codeskel pom` command is
also needed to replace raw `pom.xml` reads.

---

## Features

### 1. `annotations`: `Vec<String>` → `Vec<{name, value}>`

**Current:** `annotations: ["RestController", "RequestMapping"]`  
**New:** `annotations: [{"name": "RestController", "value": null}, {"name": "RequestMapping", "value": "/api/v1/users"}]`

The `value` field captures the default (unnamed) argument only. Named arguments and
multi-argument annotations intentionally set `value: null` in this version — named
argument capture is out of scope.

### 2. `docstring_text: string | null`

New field on every `Signature`. Contains the raw text of the preceding doc comment,
stripped of language-specific delimiters. `null` when `has_docstring` is false.
`has_docstring` is kept as-is for backward compatibility; `docstring_text.is_some()`
always equals `has_docstring`.

Strip rules per language:
- **Java/TS/JS/C/C++**: remove opening `/**`, closing `*/`; strip leading ` * ` or ` *`
  (with or without trailing space) from each interior line; trim trailing whitespace per
  line. Single-line `/** foo */` → `foo`.
- **Python**: remove surrounding `"""` or `'''` delimiters; trim leading/trailing blank
  lines from the result.
- **Go**: strip leading `// ` or `//` (no space) from each line of the immediately
  preceding comment block (no blank line between comment and declaration).
- **Rust**: strip leading `/// ` / `///` or `//! ` / `//!` from each line.
- **C#**: strip leading `/// ` / `///` from each line.
- **Ruby**: strip leading `# ` or `#` from each line of the immediately preceding comment
  block (no blank line between comment and declaration).

For Go and Ruby, "immediately preceding" follows the same definition already used by
`has_docstring` detection in those parsers — no blank line gap between the last comment
line and the declaration.

### 3. `codeskel pom` Subcommand

CLI signature:
```
codeskel pom [OPTIONS] [project_root]

Arguments:
  [project_root]            Path to the project root (default: current directory)

Options:
  --controller-path <path>  Path hint for multi-module resolution: selects the
                            sub-module whose directory contains this path
  -h, --help                Print help
```

Reads `pom.xml` without a cache. Outputs compact JSON:

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

- **`service_name`**: derived from `<artifactId>` of the resolved (sub-)module POM.
- **Multi-module**: if root `pom.xml` has `<modules>`, select the sub-module whose
  directory path contains `--controller-path`. Use that sub-module's `pom.xml`.
  Inherit `groupId` from parent when absent in module POM. Set `is_multi_module: true`.
- **Version resolution**: if `<version>` is a `${prop}` reference, resolve from module
  `<properties>` first, then parent `<properties>`. If still unresolved, output the raw
  `${prop}` string as-is.
- **`internal_sdk_deps`**: filter `<dependency>` entries where `artifactId` ends in
  `-api` or `-sdk` AND `groupId` starts with the root POM `groupId` prefix. An empty
  list is valid (not an error).
- **`existing_skill_path`**: check `docs/skills/<service_name>/SKILL.md` under the
  directory containing the resolved `pom.xml`. Return absolute path if present, `null`
  otherwise.
- **`project_root`** (for `existing_skill_path` lookup): the directory containing the
  resolved `pom.xml` file.

XML library: `roxmltree` (read-only, document-oriented, clean tree API).

**C++ attributes:** In C++, `[[nodiscard]]`, `[[deprecated("msg")]]`, etc. are the
annotation equivalent. Extract `[[attr]]` syntax: `name` is the attribute identifier,
`value` is the argument string if present (e.g. `"msg"` from `[[deprecated("msg")]]`),
otherwise `null`.

---

## Implementation Order (Option B — model-first)

1. **`models.rs`** — add `Annotation { name, value }` struct; update `Signature.annotations`;
   add `Signature.docstring_text`
2. **All 9 parsers** — populate `annotations` with name+value; populate `docstring_text`
3. **`cli.rs` + `commands/pom.rs` + `main.rs`** — new `pom` command

---

## Data Flow

```
pom.xml  ──► commands/pom.rs (roxmltree)  ──► PomOutput (stdout JSON)

source file ──► parser (tree-sitter)
                  ├── Annotation { name, value }   (annotation nodes)
                  └── docstring_text               (comment text strip)
              ──► Signature (models.rs)
              ──► FileEntry ──► cache.json ──► codeskel get
```

---

## Error Handling

### Signature enrichment
| Condition | Behavior |
|---|---|
| Annotation without default arg | `value: null` |
| Annotation with named args only | `value: null` |
| Doc comment present but empty | `docstring_text: ""` (empty string, not null) |

### `codeskel pom`
| Condition | Behavior |
|---|---|
| No `pom.xml` found anywhere | Exit 1 with message to stderr |
| Missing `artifactId` in resolved POM | Exit 1 with descriptive message |
| Missing `groupId` in all reachable POMs | Exit 1 with descriptive message |
| `${prop}` reference unresolvable | Output raw `${prop}` string as-is |
| Malformed XML | Exit 1 with parse error message to stderr |
| Permission error reading `pom.xml` | Exit 1 with IO error message to stderr |
| No matching sub-module for `--controller-path` | Exit 1 with descriptive message |
| No internal SDK deps found | Valid empty list `[]`, exit 0 |

---

## Files Changed

| File | Change |
|---|---|
| `src/models.rs` | Add `Annotation`, update `Signature` |
| `src/parsers/java.rs` | Annotation values + docstring text |
| `src/parsers/python.rs` | Docstring text |
| `src/parsers/typescript.rs` | Annotation values + docstring text |
| `src/parsers/javascript.rs` | Annotation values + docstring text |
| `src/parsers/go.rs` | Docstring text |
| `src/parsers/rust_lang.rs` | Annotation (attr) values + docstring text |
| `src/parsers/csharp.rs` | Annotation values + docstring text |
| `src/parsers/cpp.rs` | Annotation values + docstring text |
| `src/parsers/ruby.rs` | Docstring text |
| `src/cli.rs` | Add `PomArgs`, `Commands::Pom` |
| `src/commands/pom.rs` | New — full pom command implementation |
| `src/commands/mod.rs` | Export `pom` module |
| `src/main.rs` | Dispatch `Commands::Pom` |
| `Cargo.toml` | Add `roxmltree` dependency |
