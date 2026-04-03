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
multi-argument annotations set `value: null`.

### 2. `docstring_text: string | null`

New field on every `Signature`. Contains the raw text of the preceding doc comment,
stripped of language-specific delimiters. `null` when `has_docstring` is false.

Strip rules per language:
- **Java/TS/JS/C/C++**: remove `/**`, `*/`, and leading ` * ` from each line
- **Python**: remove triple-quote delimiters (`"""` or `'''`)
- **Go/Rust/C#**: strip `//`, `///`, `//!` prefix from each line
- **Ruby**: strip `#` prefix from each line

### 3. `codeskel pom` Subcommand

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

Multi-module: if root `pom.xml` has `<modules>`, select the sub-module whose directory
contains `--controller-path`. Inherit `groupId` from parent when absent in module POM.
Resolve `${prop}` version references from module then parent `<properties>`.

`internal_sdk_deps` filter: `artifactId` ends in `-api` or `-sdk` AND `groupId` starts
with the root POM `groupId` prefix.

`existing_skill_path`: check `docs/skills/<service_name>/SKILL.md` under project root.

XML library: `roxmltree` (read-only, document-oriented, clean tree API).

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

| Condition | Behavior |
|---|---|
| No `pom.xml` found | Exit 1 with message to stderr |
| Missing `artifactId`/`groupId` | Exit 1 with descriptive message |
| `${prop}` reference unresolvable | Use raw string as-is |
| Annotation without default arg | `value: null` |
| Doc comment present but empty | `docstring_text: ""` |

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
