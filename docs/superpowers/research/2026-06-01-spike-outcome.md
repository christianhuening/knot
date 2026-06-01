# Foundation Spike outcome — 2026-06-01

## Stack landed (post-pivot from Go)

| Layer | Choice | Version | Notes |
|---|---|---|---|
| Backend language | Rust (stable) | 1.96 | `rust-toolchain.toml` pin via oxalica/rust-overlay |
| Async runtime | `tokio` | 1.x | multi-thread flavor |
| HTTP / WS | `axum` | 0.7 | first-class WebSocket via `WebSocketUpgrade` |
| CRDT | `yrs` | 0.21.3 | canonical Rust Yjs |
| Markdown parser | `pulldown-cmark` | 0.12 | CommonMark + Strikethrough ext + raw HTML for `<u>` |
| Schemagen | custom JSON → Rust+TS | — | spike-decided; see `2026-06-01-schemagen-spike.md` |
| Frontend | React 18 + Vite + TS + Tiptap 2 + `y-websocket` | per spec | unchanged from pre-pivot |
| Test runners | `cargo nextest` + `vitest` + `playwright` | per spec | |

## What works

- **`yrs` 0.21.3 conformance**: 3/3 engine contract tests pass (empty doc SV; idempotent apply; two-doc convergence via state-vector exchange).
- **Markdown round-trip**: 9/9 fixtures byte-identical through both `to_markdown` and `from_markdown`:
  - paragraph, headings (1-6), blockquote, code_block, horizontal_rule, hard_break, bullet/ordered lists, marks, mixed-content.
  - All v0.1 marks round-trip including **underline** (via pulldown-cmark's `Event::InlineHtml` carrying `<u>...</u>`).
- **In-process convergence test** (`crates/knot-server/tests/convergence.rs`): two WebSocket clients connect to an in-memory broker, client A sends a Yjs update via y-sync v1 wire frame, client B receives the forwarded update.
- **Headline Playwright test** (`e2e/flows/two-users-converge.spec.ts`): two real Chromium browser contexts edit the same doc through Vite → axum → `yrs` → broadcast back, converging in well under one second. **This is the spike's GO/NO-GO gate, and it's green.**
- **Workspace test suite**: 18/18 `cargo nextest` tests pass (2 schemagen golden + 3 knot-crdt conformance + 9 knot-markdown round-trip + 2 knot-server protocol unit + 2 knot-server integration).
- **`make schema.gen`** regenerates both `crates/knot-markdown/src/schema.rs` and `web/src/features/editor/schema.ts` from `tools/schema.json`. Both are gofmt-equivalent and committed.
- **Nix dev shell** (oxalica/rust-overlay): toolchain materialised hermetically; `rustup` not used.

## yrs 0.21.3 — API findings to remember

These came up while implementing the markdown serializer and the broker. Worth capturing for Plan 5 (room actor) and beyond:

| Concern | Reality |
|---|---|
| `Doc::new()`, `Transact::transact_mut()`, `ReadTxn::transact()` | stable and intuitive |
| `Update::decode_v1`, `StateVector::decode_v1`/`.encode_v1()`, `txn.encode_state_as_update_v1(&sv)` | all where the spec assumed |
| `XmlFragmentRef::push_back(&mut txn, XmlElementPrelim::empty(...))` | returns the `XmlElementRef` directly — cleaner than insert-then-get |
| `el.tag()` | returns `&Arc<str>`, **not** `Option<&str>`. Use `.as_ref()` or `.to_string()` |
| `XmlElementRef::insert_attribute(&mut txn, name, value)` | trait `Xml` |
| `XmlText::insert_with_attributes(&self, txn, index, chunk, attributes: Attrs)` | `Attrs` is `HashMap<Arc<str>, Any>`; passed by value, not reference |
| Reading marks on text | `text.diff(&txn, YChange::identity)` returns `Vec<Diff<()>>`; each `Diff` has `.insert: Out` and `.attributes: Option<Box<Attrs>>` (the `Box` is non-obvious) |
| `Any::Map(Arc<HashMap<String, Any>>)` | the inner map uses `String` keys, not `Arc<str>` (different from `Attrs`!) |
| `Any::String(Arc<str>)` | as expected |
| `yrs::types::{Attrs, text::{YChange, Diff}}` | **not** re-exported at crate root — direct path required |

**One non-obvious yrs behaviour** (from R-T9):
> When inserting an unformatted run between formatted ones via `text.insert(pos, " ")`, the plain insertion **merges into the preceding formatted run** when read back via `diff()`. The fix is to use `insert_with_attributes(pos, text, Attrs::new())` — an empty `Attrs` forces yrs to track the run as a distinct unformatted chunk.

## pulldown-cmark 0.12 — API findings

- `Tag::BlockQuote(Option<BlockQuoteKind>)` and `TagEnd::BlockQuote(Option<BlockQuoteKind>)` — not unit variants.
- `TagEnd::List(bool)` — not `Option<u64>`.
- **Tight lists** (no blank line between items) emit `Event::Text` directly inside `Item` without a wrapping `Paragraph`. The parser must transparently introduce a paragraph wrapper to match the canonical schema.
- Code-block text always has a trailing `\n` that must be stripped before insertion (otherwise the serializer's own newline emission duplicates).
- Raw HTML comes through as `Event::InlineHtml` (small chunks) or `Event::Html` (block-level). Our `<u>...</u>` underline handler keys on `InlineHtml` only.

## Gaps / known limitations (carrying forward)

- **No Postgres persistence in the spike.** Rooms die with the process. Plan 5 adds it.
- **No auth.** Anyone with TCP reach to `:3000` can connect. Plan 3 adds local + OIDC.
- **No document tree / ACL.** Plan 4.
- **No actor model in the broker.** R-T12 uses per-room `tokio::sync::Mutex` for simplicity. Plan 5 replaces with the actor-per-room pattern from spec §8.3. The `crates/knot-server/src/room.rs` module has a comment pointing at this.
- **No production observability.** stdout `tracing` only; no Prometheus, no OTLP. Plan 2 wires them.
- **No Helm / Docker / CI.** Plans 2 and 9.
- **No CollaborationCursor.** Tiptap's presence cursor is intentionally omitted in the spike to keep convergence simple. Plan 7 adds it with a correctly-timed provider lifecycle.

## Foundation spec edits required

None. The spec already reflects all the architectural choices reality validated. The yrs and pulldown-cmark API findings above are tactical implementation notes that don't change any decision in the spec.

## Performance smell-check

- `cargo nextest run --workspace` on a warm cache: <0.1 s total.
- Cold `cargo build` for `knot-server`: ~10-15 s.
- Playwright headline test wall-clock: ~1.6 s (warm server binary, including Vite startup).
- In-process convergence test (two ws clients, one update forwarded): <100 ms total.

No optimisation needed for v0.1 scale.

## Verdict

**GO.** The spike's load-bearing assumptions are all validated:

1. ✅ A Rust Yjs implementation (yrs) drives full ProseMirror XmlFragment editing end-to-end.
2. ✅ The canonical JSON schema feeds two-language codegen cleanly.
3. ✅ Markdown round-trip is lossless for the v0.1 schema, including underline.
4. ✅ Two real browsers converge through an axum WebSocket broker speaking y-sync v1.

Proceed to **Plan 2 (Repo bootstrap & DB)** with confidence in the foundation.

## Commit trail (master)

```
b3821dc test(e2e): playwright headline convergence test (two browsers)
f505d4c feat(web): wire Tiptap editor to spike server via y-websocket
4ae7876 feat(knot-server): spike y-sync v1 broker over axum WebSocket
42bf111 test(knot-markdown): mixed-content round-trip fixture
ac28695 feat(knot-markdown): parse markdown via pulldown-cmark; round-trip suite
321ae83 feat(knot-markdown): serialize inline marks (bold/italic/code/strike/underline/link)
21cc076 feat(knot-markdown): serialize blockquote, code_block, hr, hard_break, lists
1705722 feat(knot-markdown): serialize paragraph and heading
fb4a47d feat(knot-crdt): Engine trait + yrs-backed implementation
58c6a1b feat(web): bootstrap Vite + React + TS app + generated schemas
ea94b3d feat(schemagen): emit TypeScript schema constants
d07f8e6 feat(schemagen): emit Rust schema constants from tools/schema.json
fdbb4de feat: declare canonical ProseMirror schema as data
8766649 chore: bootstrap Cargo workspace + Makefile + Rust dev tooling
f5a3627 research: schemagen approach decision spike — JSON-canonical wins
1a5bd3a docs: pivot Foundation spec + Plan 1 from Go to Rust
```

Plus `archive/go-attempt` (8 commits) for historical reference.
