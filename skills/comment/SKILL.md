---
name: comment
description: >
  Generates accurate, complete docstrings for under-commented codebases by processing files
  in dependency order — so each file is commented with full knowledge of what it depends on.
  Edits source files directly in place (safe to run on git-tracked projects).

  Use this skill whenever the user wants to add comments, docstrings, or documentation to
  source code — whether it's a single file, a module, a directory, or an entire project.
  Trigger on phrases like: "add comments to", "generate docstrings for", "document this
  code", "this project has no comments", "add javadoc", "comment my code", "write docs for".
  Even if the user just says "this code is hard to understand" and points at a file or project,
  this skill is appropriate.
---

# Comment Skill

You are generating docstrings for a codebase. The goal is **accuracy and completeness**, not
verbosity — the comments will be read primarily by AI agents, so they need to be precise and
informative, not ornate.

## Prerequisites: check for `codeskel`

Before anything else, verify `codeskel` is available:

```bash
which codeskel
```

If not found, stop and tell the user:

```
codeskel is required but not installed.
Install it with:  cargo install codeskel
Then re-run this skill.
```

## Determine scope

The user may specify a whole project, a specific directory, or a single file. If not clear, ask.
Examples:
- "comment my project at ~/code/myapp" → project root
- "add docstrings to src/service/UserService.java" → single file
- "document everything under src/main/java/com/example/api" → directory

## Workflow

### For a single file

Skip the scan/loop. Go straight to [Commenting a file](#commenting-a-file) with just that file.
No `codeskel` needed for a single file.

### For a directory or whole project

**Step 1 — Scan**

```bash
codeskel scan <project_root_or_dir>
```

This outputs a compact JSON summary to stdout:

```json
{
  "cache": "/path/to/.codeskel/cache.json",
  "stats": { "to_comment": 42, "skipped_covered": 8, "skipped_generated": 3, "total_files": 53 }
}
```

Tell the user: "Found N files to comment, skipping M already-covered and K generated files."

If `to_comment` is 0, tell the user everything already has sufficient documentation and stop.

**Step 2 — Loop through files in dependency order**

For `i = 0` to `stats.to_comment - 1`:

```bash
# Get this file's details
codeskel get <cache_path> --index <i>

# Get signatures of its direct dependencies (context for commenting)
codeskel get <cache_path> --deps <file_path>
```

Then [comment the file](#commenting-a-file), write it back, and update the cache:

```bash
codeskel rescan <cache_path> <file_path>
```

Print progress: `[12/42] src/main/java/com/example/service/UserService.java`

---

## Commenting a file

This is where the actual work happens. For each file:

**1. Load context**
- Read the full source file
- If using `codeskel`: you already have the file's `signatures` and its dependencies' signatures from `codeskel get --deps`. Hold these as context — they tell you what imported types actually do, which is often the key to writing accurate docstrings.
- If `cycle_warning: true`: the file has a circular dependency. Read both files before commenting either.

**2. Identify what needs documentation**

Documentable items (add a docstring if `has_docstring: false`):
- Classes, interfaces, enums, annotations
- Public and protected methods and constructors
- Public and protected fields (especially important for API/SDK types where field semantics matter to callers)

Skip: private fields, auto-generated getters/setters with obvious semantics (e.g. `getId()`, `setName()`), test methods.

**3. Write the docstrings**

Style guide per language:

| Language | Style |
|---|---|
| Java | Javadoc: `/** ... */` with `@param`, `@return`, `@throws` |
| Python | Google-style docstrings with Args/Returns/Raises sections |
| TypeScript / JavaScript | JSDoc: `/** ... */` with `@param`, `@returns` |
| Go | Standard Go doc comments: `// FuncName ...` |
| Rust | `/// ...` line doc comments |
| C# | XML doc comments: `/// <summary>`, `/// <param>` |
| C/C++ | Doxygen: `/** ... */` with `\param`, `\return` |
| Ruby | YARD: `# @param`, `# @return` |

**What to write:**

For a method/function:
- One-sentence summary of what it does (not how)
- `@param` for each non-obvious parameter — describe its meaning, not just its type
- `@return` describing what the return value represents
- `@throws`/`@raises` for any exception that callers should handle
- Any non-obvious preconditions or side effects

For a class/interface:
- What this type represents in the domain
- Its primary responsibility and lifecycle (e.g. "Created by X, used by Y")
- Any important invariants

For a field:
- What this value represents, its valid range or constraints if non-obvious
- Why it exists (especially for flags, counters, caches)

**What not to write:**
- Don't restate the method name ("Gets the user" for `getUser()`)
- Don't document obvious parameters (`@param id The id`)
- Don't add inline comments — docstrings only
- Don't pad with filler sentences

**4. Edit the file**

Insert docstrings directly into the source file. Preserve all existing code exactly — only add doc comment blocks. Do not reformat, rename, or restructure anything.

Use the Edit tool to insert each docstring immediately before its declaration.

**5. Verify**

After editing, read back a sample of the modified file to confirm:
- Docstrings are syntactically correct for the language
- No existing code was accidentally modified
- Comment blocks are properly closed

---

## Handling edge cases

**Large file (>500 lines):** Read it in sections. Comment in multiple Edit passes.

**Heavily coupled class (imports many things):** The dep signatures from `codeskel get --deps` are your friend. Read them carefully before writing anything — the parameter types are only meaningful if you know what those types do.

**Ambiguous method:** If you genuinely can't determine what a method does from its body and dependencies, write a concise docstring describing what you can observe (inputs, output type, what it calls), and note "Behavior may depend on [X]" rather than guessing.

**Already has partial docstrings:** Keep existing docstrings. Only add where `has_docstring: false` (from `codeskel get` output). Don't overwrite or merge — that risks losing intentional human-written documentation.

---

## Finishing up

After completing all files:

1. Print a summary: "Commented N files. M items documented."
2. Remind the user: "All changes are unstaged. Run `git diff` to review before committing."
3. If any files had `cycle_warning: true`, mention them: "Note: X files had circular dependencies — review their docstrings for cross-references."
