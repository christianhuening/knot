# Schemagen approach — decision spike (2026-06-01)

## Context

knot's Rust backend and TypeScript frontend share a small, stable schema: ~11 ProseMirror node kinds, ~6 mark kinds, and per-mark Markdown-serialisation metadata. Both languages need the same names and metadata at compile time. The question is whether to reach for an off-the-shelf Rust→TS codegen crate, build a custom JSON-driven schemagen binary (what Plan 1 currently proposes), or hand-author both sides with a sync test.

---

## Candidates evaluated

### 1. ts-rs

- **Crate:** ts-rs v12.0.1, last release 2026-01-31
- **License:** MIT
- **API:** `#[derive(TS)]` proc-macro on Rust types; `#[ts(export)]` triggers a test that writes `.ts` files when `cargo test` is run. Export directory configured via `TS_RS_EXPORT_DIR` env var or `.cargo/config.toml`. Alternatively call `TS::export_all()` / `TS::export_to_string()` programmatically.
- **Fit for NodeKind/MarkKind enums:** Good. ts-rs generates TypeScript string literal unions for unit enums (all variants, no data) — i.e. `type NodeKind = "doc" | "paragraph" | ...`. The `#[ts(repr(enum))]` attribute can switch to a TypeScript `enum` instead if desired. Serde `rename_all` attributes are respected.
- **Fit for MarkSerialization const table:** Poor. ts-rs is a *type* exporter, not a *value* exporter. There is no mechanism for emitting a `const MARK_SERIALIZATION: Record<MarkKind, MarkMarkdownMeta> = { ... }` object from a Rust data structure. A Rust `const` or static array could be exported as a type annotation, but not as a literal value table. The `MarkMarkdownMeta` struct and the enum types could be exported cleanly; a parallel handwritten const table in TS would still be required.
- **Adoption:** ~1,800 GitHub stars; active maintenance (v11→v12 in 2025-2026); MSRV 1.88.
- **Verdict for knot:** Handles the enum type exports well, but the const metadata table (the `mark_serialization` function / `MARK_SERIALIZATION` object) cannot be generated from it. You would end up with ts-rs for types *plus* a hand-maintained TS const table, giving you two files to keep in sync instead of one. Doesn't fully solve the problem.

---

### 2. typeshare

- **Crate:** typeshare v1.13.4, last release ~December 2025
- **License:** Apache-2.0 OR MIT (dual)
- **API:** `#[typeshare]` attribute proc-macro on Rust types; a separate CLI binary (`typeshare-cli`) is invoked (e.g. via Makefile) with a source directory and output path. Works at the CLI level, not via `cargo test`.
- **Fit for NodeKind/MarkKind enums:** Partial, but with a significant design mismatch. typeshare generates TypeScript `const enum` declarations (e.g. `export const enum NodeKind { Paragraph = "paragraph", ... }`). TypeScript `const enum`s have well-known ergonomic problems: they are inlined by the compiler and can behave unexpectedly with isolatedModules / Vite / esbuild toolchains (which are all present in knot's frontend). The modern TS community preference — and Tiptap's convention — is string literal unions, not `const enum`. This is a concrete portability risk.
- **Fit for MarkSerialization const table:** None. typeshare is a type-only exporter; it cannot emit value tables.
- **Adoption:** ~2,900 GitHub stars; maintained by 1Password; last release December 2025.
- **Verdict for knot:** The `const enum` output is a poor fit for a Vite/esbuild frontend. The const table problem is identical to ts-rs — still unsolvable. Two strikes.

---

### 3. specta

- **Crate:** specta v2.0.0-rc.25 (core), specta-typescript v0.0.11, last specta-util release ~May 2026. **v2.0 has not reached stable.**
- **License:** MIT
- **API:** `#[derive(Type)]` proc-macro; standalone export via `specta_typescript::export::<MyType>(&ExportConfig::default())` — no Tauri or rspc required. Build.rs or a dedicated tool binary can call the export function programmatically. The most common pattern in 2025-2026 is a `build.rs` that writes out a `.ts` file at build time.
- **Fit for NodeKind/MarkKind enums:** Good. specta auto-detects "string enums" (all unit variants) and emits TypeScript string literal unions: `type Status = "Active" | "Inactive" | "Pending"`. No serde attribute required; behaviour is automatic.
- **Fit for MarkSerialization const table:** None. specta is also a type-only exporter; value tables are outside its scope.
- **Adoption:** ~595 GitHub stars (significantly lower than ts-rs/typeshare). v2.0 still in RC after 25 release candidates — the instability signal is real. Funded by Flight Science, development is active, but the long RC cycle is a maintenance risk for a new project that should not be holding an unstable dep at its foundation.
- **Verdict for knot:** Better enum ergonomics than typeshare, but the const table problem is identical, and the still-unstable v2.0 is an additional concern for a greenfield project that needs a solid base.

---

### 4. Custom JSON-canonical + custom schemagen

*(No web research; analysed from first principles against the Plan 1 tasks already written.)*

- **Crate:** none — `tools/schemagen` is a ~200-line Rust binary in the workspace.
- **License:** n/a (owned code, Apache-2.0 same as the project).
- **API:** `cargo run -p schemagen -- --lang rust --out ...` or `--lang ts --out ...`. Called from `make schema.gen` and a pre-commit hook. Outputs are committed generated files with a `// Code generated by tools/schemagen. DO NOT EDIT.` header. TDD via golden-file tests.
- **Fit for NodeKind/MarkKind enums:** Perfect. The emitter is purpose-built to emit exactly `export const NODE_KINDS = ["doc", "paragraph", ...] as const; export type NodeKind = (typeof NODE_KINDS)[number];` — the modern idiomatic TS pattern with a runtime-accessible array.
- **Fit for MarkSerialization const table:** Perfect. The emitter already handles `MarkMarkdownMeta` and the full `MARK_SERIALIZATION` const table in both Rust and TypeScript. This is the use-case the custom tool was built for.
- **Maintenance burden:** ~200 lines of straightforward string-concatenating Rust. The golden-file tests mean a change to the schema JSON causes a deterministic test failure and a one-command golden update. The schemagen binary itself has no external deps beyond `serde_json` and `clap`, both of which are already in the workspace.
- **Adoption:** N/A (internal tool). The pattern (codegen from JSON + golden tests) is standard and well-understood.
- **Forward-compat:** Adding a new node or mark = one line in `tools/schema.json`, then `make schema.gen` + `git add`. Single commit, zero manual edits to generated files.
- **Verdict for knot:** Solves the entire problem — enum types *and* const metadata — in a single tool, with no external dep, and Plan 1 already specifies and implements it.

---

### 5. Hand-author both sides + sync test

*(No web research; analysed from first principles.)*

- **Fit for NodeKind/MarkKind enums:** Requires manually maintaining two files that must agree.
- **Fit for MarkSerialization const table:** Requires manually maintaining two files that must agree.
- **Maintenance burden:** High. Every new node or mark requires editing `schema.rs`, `schema.ts`, and the test. Three-file changes are easy to forget. The sync test catches drift after the fact (when CI runs), not at authoring time.
- **Forward-compat:** Acceptable but error-prone. The test catches mismatches but doesn't prevent them.
- **Verdict for knot:** Strictly worse than option 4 in every dimension — more files to change, more chance of silent drift, no single source of truth. The only advantage is zero tooling overhead, which is irrelevant since option 4 requires only ~200 lines that already exist in the plan.

---

## Decision matrix

| Criterion | ts-rs | typeshare | specta | JSON-canonical | Hand+test |
|---|---|---|---|---|---|
| Single source of truth | Partial — types only, const table still hand-maintained | Partial — types only, const table still hand-maintained | Partial — types only, const table still hand-maintained | **Yes** — one JSON file drives both outputs | No — two files + test |
| MD metadata fit | No | No | No | **Yes** — first-class in emitter | Manual |
| Maintenance burden | Low (maintained crate) but leaves a gap | Low (maintained crate) but leaves a gap + const enum risk | Medium (v2.0 RC instability) and leaves a gap | **Low** (~200 lines, golden-tested, already written) | High |
| Static enum fit | Good (string union) | Poor (const enum, esbuild incompatible) | Good (string union) | **Good** (emits array + union pattern) | Manual |
| Forward-compat | Partial — add node in Rust, but still update TS const table | Partial — add node in Rust, but still update TS const table | Partial — add node in Rust, but still update TS const table | **Full** — add one line to schema.json | Poor |

---

## Recommendation

**Winner: Option 4 — Custom JSON-canonical + custom schemagen.**

Reasons (in order of weight):

1. **It is the only option that covers the full requirement.** Options 1-3 are type exporters. They can emit `type NodeKind = ...` from Rust, but none can emit a const value table like `MARK_SERIALIZATION`. Every off-the-shelf crate leaves a gap that must be filled by hand, defeating the single-source-of-truth goal for precisely the part of the schema that changes most often (mark metadata).

2. **The implementation is already specified and essentially written.** Plan 1 Tasks 2-4 contain the full implementation with TDD golden tests. Adopting ts-rs/typeshare/specta would require *deleting* that work and accepting a partial solution.

3. **The TS output pattern is purpose-built for the use case.** The emitter produces `export const NODE_KINDS = [...] as const; export type NodeKind = (typeof NODE_KINDS)[number];` — a runtime-accessible array plus a derived type. This is more useful than any of the off-the-shelf tools' output because the editor and the MD serializer both need *runtime access* to the list, not just a compile-time type.

4. **Zero unstable dependencies.** specta v2.0 is still RC. typeshare's `const enum` output is a concrete esbuild/Vite incompatibility risk. ts-rs is stable but incomplete for the use case. The custom tool has no external deps beyond `serde_json` and `clap`.

**Risks / accepted tradeoffs:**

- The ~200 lines of schemagen code are owned code — bugs in the emitter are bugs knot must fix. This is acceptable given the simplicity of the logic and the golden-file test coverage.
- If the schema grows significantly (e.g. 50+ node types, complex attribute metadata), a more declarative schema language (e.g. JSON Schema or protobuf) might become attractive. At v0.1 scale this is not a real concern.
- ts-rs/typeshare/specta may improve to support value tables in future — worth re-evaluating if the schema significantly expands. For now, they do not solve the problem.

---

## Implications for Plan 1

**No changes required to Tasks 2-4.** The custom JSON-canonical approach they specify is the correct answer. Specifically:

- Task 2 (define `tools/schema.json`) is correct as written. The JSON file is the canonical single source of truth.
- Task 3 (schemagen Rust emitter) is correct as written. The golden-file test pattern and the `NodeKind` / `MarkKind` / `MarkMarkdownMeta` / `mark_serialization` output match the requirements exactly.
- Task 4 (schemagen TypeScript emitter) is correct as written. The `NODE_KINDS as const` + derived union type + `MARK_SERIALIZATION` record pattern is idiomatic and esbuild/Vite-compatible.

One clarification worth adding to Task 3/4: the generated files should be committed to the repository (they are — the plan already does `git add web/src/features/editor/schema.ts`), and the pre-commit hook mentioned in §8.8 of the Foundation spec should call `make schema.gen` and then `git add` the generated files. This prevents schema drift from being committed silently.
