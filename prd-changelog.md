# PRD Changelog

## 2026-04-03 — `codeskel pom` subcommand + signature enrichment

**Context:** `generate-microservice-skill` needed structured project metadata without
loading raw files into LLM context. Two gaps were identified in `codeskel get` output:
Javadoc text was not extracted (only a boolean), and annotation argument values were
not captured (only annotation names).

### Added

- **`codeskel pom` subcommand** — reads `pom.xml` without a cache and outputs a compact
  JSON object with `service_name`, `group_id`, `version` (property references resolved),
  `internal_sdk_deps` (filtered `-api`/`-sdk` dependencies matching root groupId prefix),
  and `existing_skill_path`. Supports multi-module Maven projects via `--controller-path`
  hint; inherits `groupId` from parent POM when absent in module POM.

- **`docstring_text: string | null`** field on every signature item — the raw text of
  the preceding doc comment, stripped of `/** */` delimiters and leading ` * `.
  `null` when `has_docstring` is false.

- **`annotations` changed from `string[]` to `{name, value}[]`** — each entry now
  carries the annotation name (without `@`) and its default (unnamed) argument value,
  or `null` if the annotation takes no arguments. Enables extracting
  `@RequestMapping("/api/v1/users")` path values without reading raw source.

- **"How the `generate-microservice-skill` uses codeskel" section** — documents the
  3-call flow (`codeskel pom` → `codeskel scan` → `codeskel get`) that replaces raw
  `pom.xml` and Java source file reads in Steps 1–2 of that skill.

- **Overview** updated to list both primary consumers (`comment` skill and
  `generate-microservice-skill`).

---

## 2026-04-02 — Initial PRD

Original specification covering `codeskel scan`, `codeskel get`, `codeskel rescan`,
tree-sitter parsing, dependency graph, comment coverage calculation, signature
extraction, generated file detection, cache format, and performance requirements.
