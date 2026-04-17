Here are specific observations from this session, grouped by where the fix belongs:

---

## `codeskel` tool suggestions

**1. Separate rescan from advance**

The biggest footgun I hit: `codeskel next` conflates "trigger rescan of previous file" with "advance to next file". When I called `next` twice in a row (once to print a summary, once to print details), I silently skipped `Query.java`. A dedicated `codeskel rescan <file>` subcommand — or a `--no-advance` flag — would decouple these concerns entirely. The agent edits a file, then explicitly rescans it, then calls `next` to advance. Zero ambiguity.

> **Response:** `codeskel rescan <file>` already exists as a standalone subcommand. The tool-side concern is already addressed. The double-`next` footgun is a comment skill guidance problem, not a tool problem — the skill needs to enforce one `next` call per iteration.

**2. Output size control for large files**

The first dep (`AppExCode.java`) produced **43 KB** of JSON — hundreds of error-code constants with full `docstring_text` payloads. codeskel had to save it to disk. For files with many fields (constants classes, large entities), a `--max-signatures <N>` or `--compact` flag that truncates field-level signatures after N entries would cut this dramatically. Fields on constants classes are rarely the high-value commenting targets anyway.

> **Response:** Implementing as kind-based field truncation rather than a flat `--max-signatures` flag. A `--max-fields N` option (default 5) will truncate only `field`-kind signatures per dep entry, keeping all methods, constructors, and type declarations intact. Truncated entries will include a `fields_omitted` count so the agent knows the class shape without the full list. See spec `2026-04-17-output-size-and-docstring-metadata-design.md`.

**3. Target-scoped scanning**

`codeskel scan` currently scans all 1851 files to build the dep graph for a single-file target. A `--target <file>` flag on scan that only resolves that file's transitive closure would make targeted mode much faster and keep `to_comment` honest (currently 1814 is misleading — only ~25 are relevant).

> **Response:** Deferred. Scan is a one-time operation per project; the effort of scoping it to a transitive closure is not justified. The `to_comment` misleading count is a display issue only — targeted mode already operates on the correct ~25-file chain. Not implementing.

**4. Richer `has_docstring` metadata**

When `--min-docstring-words 30` is used, `has_docstring: false` means two different things: "no doc at all" vs "existing doc is too thin". The agent has to read the source to distinguish them. Add `existing_word_count: N` or `stub_only: true` so the agent can immediately decide "add from scratch" vs "improve existing" without a file read.

> **Response:** Implementing `existing_word_count: usize` on `Signature` (and `DepSignature`). Always present; `0` when no docstring, `N` when docstring exists but may be below threshold. The combination of `has_docstring: false` + `existing_word_count > 0` unambiguously signals "thin doc, improve existing". See spec `2026-04-17-output-size-and-docstring-metadata-design.md`.

**5. Dep chain depth control**

25 transitive deps for a single controller file included things like `EstateBatchDetails` and `StatusDevicePageDto` that had zero bearing on improving the target's comments. A `--max-depth <N>` option for `next --target` (defaulting to 2 or 3) would focus effort on the deps that actually shape the target's contract.

> **Response:** Not implementing. Capping depth would silently exclude files from the commenting chain, leaving them permanently uncommented — which defeats the purpose. The refs analysis already filters dep *signatures* to referenced symbols, addressing the noise problem at the token level. The field truncation from suggestion #2 further reduces token weight for deep, field-heavy deps. Chain length (25 files) is a session-duration concern, not a token-budget one.

**6. Session state isolation**

The current session state lives inside `cache.json`, making it unclear whether a stale session will corrupt a fresh scan. A separate `codeskel session` file (or `--session <path>`) would make it safe to run multiple targeted sessions in parallel and easy to `codeskel reset-session` after an accidental double-`next`.

> **Response:** Already done. Session state lives in `session.json`, separate from `cache.json`. A fresh scan overwrites `cache.json` without touching the session, and `session.json` can be deleted independently to reset. No further work needed.

---

## Comment skill suggestions

> **For the comment skill maintainer — action summary:**
> All 5 suggestions below are valid and should be implemented in the skill. The tool changes (--max-fields, existing_word_count) are being shipped separately and will make some of these easier; the skill can start using them immediately once available. Key facts from the tool side:
> - `codeskel rescan <file>` already exists — agents should use it explicitly instead of relying on `next` to trigger rescan implicitly.
> - `next` output will soon include `fields_omitted: N` on dep entries where fields were truncated — useful for the tiered treatment in suggestion #4.
> - `existing_word_count` will be added to all signatures — use it to drive "write from scratch" vs "improve existing" decisions without reading source files.
> - `codeskel peek` does NOT exist — the recovery pattern for large truncated output is: read the saved JSON file path shown in the truncation message.

**1. Guard against double-`next` with a single shell call**

The skill instructs calling `codeskel next` to get file context, but the natural agent pattern of "first call for summary, second call for details" silently skips a file. The skill should enforce: **one `next` call per iteration, period**. Pipe summary extraction into the same call using Python/jq inline. Never call `next` twice without an edit in between.

> **Response:** Valid. The skill should add an explicit rule: one `next` call per iteration. If the output is large, parse/read it from the same call result — never call `next` again for details. Left for comment skill maintainer to implement.

**2. Add explicit "peek before advance" pattern for large outputs**

When the first `next` response is large, the agent loses the content (saved to file). The skill should add a pattern for this: use `codeskel peek` (see above) or parse the saved file path from the truncation message and `Read` it — this should be a named step in the workflow, not something the agent improvises.

> **Response:** Valid. The field truncation from tool suggestion #2 will reduce how often this occurs (constants files will no longer overflow), but large target files can still trigger it. The skill should name a "read saved output" recovery step explicitly. Note: `codeskel peek` does not exist as a tool command — the pattern should be: if output is truncated, `Read` the saved JSON path that appears in the truncation message. Left for comment skill maintainer.

**3. Skip threshold for constant/field-heavy dep files**

The skill should add a rule: if a dep file contains **only fields with `has_docstring: false`** (no classes or methods), and all fields already have some `docstring_text`, skip writing and call `next` immediately. Pure constants classes (`AppExCode`-style) don't benefit from per-field improvement passes the way service interfaces do.

> **Response:** Partially addressed by tool change — with `--max-fields 5`, constants classes now emit minimal output so processing them costs little context. The skip rule is still valid as a further optimization. After tool change #2 ships, assess whether the skip rule is still needed in practice. Left for comment skill maintainer.

**4. Token budget guidance per dep type**

The skill currently treats all dep files equally. A tiered approach would save significant context:
- **Service interfaces** (most tokens): full treatment, read all signatures
- **Entity/DTO classes**: improve class-level doc only if thin, skip obvious field docs
- **Constants classes**: class-level doc only
- **Utility classes**: methods only, skip fields

> **Response:** Valid direction. The `existing_word_count` field (tool change #4) will make the "thin vs. absent" decision explicit without file reads, enabling this tiered approach. The tiering logic itself belongs in the skill. Combined with field truncation, constants classes will naturally show up as "class header + 5 fields + fields_omitted: N" — the skill can use that signal directly. Left for comment skill maintainer.

**5. Make the `--min-docstring-words` choice more opinionated**

Currently the skill says "use 30 for improvement passes" but doesn't warn that on a large project this will flag thousands of files. Add a recommendation to combine `--min-docstring-words` with `--include` glob patterns when in targeted mode, so only the relevant package gets re-scanned with the higher threshold.

> **Response:** Valid. The skill should warn that `--min-docstring-words` on a full project scan dramatically increases `to_comment` count, and recommend pairing it with `--include` when in targeted mode. Left for comment skill maintainer.