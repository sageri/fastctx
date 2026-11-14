# Host compatibility

Notes on running FastCtx inside specific agent hosts. Each entry records the symptom, where the responsibility lies, the verified workaround, and the exact versions and dates it was verified on.

## zcode: the built-in Edit tool does not count FastCtx reads

**Symptom.** A file read successfully with `inspect_local_file` — complete or paged, content fully returned — still fails the built-in Edit tool:

```text
File has not been read yet. Read it first before writing to it.
```

**Attribution.** Host-side integration limitation; not a FastCtx defect. In the verified zcode 3.18.2 bundled GLM engine, the Edit precondition consults an internal read-state registry populated by built-in tools and internal subsystems. The tested implementation does not register MCP read results in that registry, so a successful FastCtx read does not satisfy its Edit precondition. Details and investigation record: [issue #4](https://github.com/sageri/fastctx/issues/4).

**Mitigations (verified).**

1. Before calling the built-in Edit, read the target file once with the built-in Read tool; reading only the target region with `offset`/`limit` is enough — the gate does not require the whole file or even the edit target. FastCtx reads remain the preferred way to explore content. (verified 2026-11-14)
2. `old_string` must match the Read output character-for-character, including full-width versus half-width punctuation; a mismatch surfaces as `String to replace not found`. (verified 2026-09-03)

**Caveat.** Do not read a large file whole with the built-in Read just to satisfy the gate: a read that comes back token-truncated (partial view) does not satisfy the gate either, and the host refuses whole-file reads above its size cap (observed at ~256 KiB). Read the bounded region you plan to edit.

**Verified on.** zcode desktop 3.18.2 (`@zcode/desktop`) with its bundled GLM engine, FastCtx 0.2.6, Windows — 2026-11-14, via a seven-scenario integration matrix (unread / FastCtx complete / FastCtx paged / built-in Read complete / paged / out-of-range / token-truncated → Edit). First observed 2026-08-29 (FastCtx version at the time not recorded). Engine bundle SHA-256 `08726b5e…b2c`.

## Codex: `apply_patch` has no read precondition

Codex's `apply_patch` applies to a file regardless of which tool read it — including files never read at all. This is a different editing-API contract, not a controlled cross-model comparison. (checked 2026-11-14 during the same investigation; record in [issue #4](https://github.com/sageri/fastctx/issues/4))
