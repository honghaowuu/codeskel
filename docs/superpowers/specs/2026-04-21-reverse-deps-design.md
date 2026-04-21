# Design: Reverse Dependency Context for Interfaces, Abstract Classes, and Annotations

## Problem

The dependency graph sorts files by compile-time imports ("A imports B → B comes first"). This works well for regular classes but fails for **context-poor kinds** — files that are depended on by many but import few:

- **Interfaces** — `UserRepository` has no imports; its meaning comes from `JpaUserRepository implements UserRepository`
- **Abstract classes** — `AbstractBaseService` is understood through its concrete subclasses
- **Annotation definitions** — `@Audited` is understood from the classes/methods that apply it

When the `comment` skill calls `--deps` before documenting these files, it gets an empty or sparse result, leaving the LLM without meaningful context.

## Goal

Surface reverse-dependency signatures (implementors, subclasses, annotation usage sites) automatically when `--deps` or `next` is called on a context-poor file kind. No skill changes required for the core loop; the skill gets richer context transparently.

---

## Section 1: Data Model

Two new fields added to each file entry in `cache.json`:

**`file_kind: String`** — the primary kind of the file's top-level declaration. Values: `"class"`, `"interface"`, `"abstract_class"`, `"annotation"`, `"enum"`, `"other"`. Derived from the first top-level signature during scan.

**`reverse_deps: Vec<String>`** — file paths that declare `implements`, `extends`, or `@ThisAnnotation` pointing at this file. Distinct from `internal_imports` (which tracks import statements only).

Example:
```json
{
  "path": "src/main/java/com/example/repo/UserRepository.java",
  "file_kind": "interface",
  "reverse_deps": [
    "src/main/java/com/example/repo/JpaUserRepository.java",
    "src/main/java/com/example/repo/MockUserRepository.java"
  ],
  "internal_imports": [],
  "signatures": [ ... ]
}
```

---

## Section 2: Scan Phase Changes

### Parse `implements`/`extends`/annotation-use clauses

After parsing import statements, a second pass over each file extracts:
- `implements SomeInterface` → records `SomeInterface` as a reverse-dep relationship
- `extends AbstractClass` → same
- `@SomeAnnotation` on a class or method → same for annotation files

These are resolved to file paths using the same package+class-name matching already used for `internal_imports`. Same-package references (no import statement needed) are resolved via the package namespace built during scan.

### Build `reverse_deps` via post-pass inversion

After all files are parsed, invert the collected map: for each `(file → implements InterfaceFile)`, append `file` to `InterfaceFile.reverse_deps`. This post-pass runs after full scan since the complete picture is needed before inverting.

### Derive `file_kind`

After signatures are extracted, set `file_kind` from the first top-level signature's `kind`. Mapping:
- signature `kind == "interface"` → `file_kind = "interface"`
- signature `kind == "class"` with `"abstract"` in modifiers → `file_kind = "abstract_class"`
- signature `kind == "annotation"` → `file_kind = "annotation"`
- otherwise → `file_kind = "class"` (or `"enum"`, `"struct"`, etc.)

### Topo sort is unchanged

`DepGraph` and topological ordering continue to follow compile-time import deps. `reverse_deps` is metadata only.

### Language scope (initial)

Java only. Python `ABC`/`Protocol`, TypeScript interfaces, Go interfaces are follow-on work.

---

## Section 3: `--deps` and `next` Output Changes

### Trigger condition

When `file_kind` is `"interface"`, `"abstract_class"`, or `"annotation"`, include reverse-dep signatures in the response.

### Shared implementation

`get --deps` and `next` both call `build_deps`. Enhancing `build_deps` benefits both commands automatically.

### Response shape

A new `reverse_deps` field is added alongside `deps` (separate field, not merged, so consumers can distinguish import-based vs. reverse-dep context):

**`get --deps` response:**
```json
{
  "dep_signatures": [],
  "reverse_dep_signatures": [
    {
      "path": "src/main/java/com/example/repo/JpaUserRepository.java",
      "signatures": [ ... ]
    }
  ]
}
```

**`next` response:**
```json
{
  "done": false,
  "index": 3,
  "remaining": 8,
  "file": {
    "path": "src/main/java/com/example/repo/UserRepository.java",
    "file_kind": "interface",
    ...
  },
  "deps": [],
  "reverse_deps": [
    {
      "path": "src/main/java/com/example/repo/JpaUserRepository.java",
      "signatures": [ ... ]
    }
  ]
}
```

`reverse_deps` is omitted (or empty array) for regular classes — zero overhead.

### Cap

Up to **5 reverse deps** included, sorted by path for determinism. Prevents flooding LLM context when an interface has dozens of implementors. Configurable via `--max-reverse-deps` as a future extension.

### Skill impact

The `comment` skill passes `reverse_deps` into the LLM prompt when present alongside `deps`. This is a small prompt-template addition; the core loop (`codeskel next` calls) is unchanged.

---

## Acceptance Criteria

- [ ] `file_kind` is present on all file entries in `cache.json`
- [ ] `reverse_deps` is populated for interfaces, abstract classes, and annotations after scan
- [ ] Same-package `implements`/`extends` relationships are resolved (no import statement required)
- [ ] `get --deps` on an interface returns `reverse_dep_signatures` with up to 5 implementor entries
- [ ] `next` on an interface file returns `reverse_deps` field populated
- [ ] `get --deps` and `next` on a regular class return empty/absent `reverse_deps` (no regression)
- [ ] Topo sort order is unchanged
- [ ] Java parser updated; other parsers unchanged for now
