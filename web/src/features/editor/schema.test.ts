/**
 * Asserts the live Tiptap/ProseMirror schema's node + mark names match the
 * canonical snake_case set declared in `tools/schema.json` (generated to
 * `schema.ts`). Without this, individual extensions can ship in camelCase
 * by default and we only notice when markdown export blows up at runtime
 * (cf. the `UnsupportedNode("bulletList")` regression).
 */

import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import * as Y from "yjs";
import { Awareness } from "y-protocols/awareness";

import { createExtensions } from "./extensions";
import { NODE_KINDS, MARK_KINDS } from "./schema";

describe("editor schema alignment", () => {
  it("every Tiptap node maps to a snake_case kind from tools/schema.json", () => {
    const doc = new Y.Doc();
    const awareness = new Awareness(doc);
    const editor = new Editor({
      extensions: createExtensions({
        doc,
        awareness,
        user: { name: "test", color: "#000" },
      }),
    });
    // Every node the schema generator declared must be present at the same
    // name on the live PM schema.
    for (const kind of NODE_KINDS) {
      expect(editor.schema.nodes[kind], `node ${kind} missing from PM schema`).toBeDefined();
    }
    // And every PM node must be one of the declared kinds — no camelCase
    // leak-through from a yet-to-be-renamed extension. `doc` and `text` are
    // always present even when not in NODE_KINDS in some shapes; check both
    // directions but allow the implicit ProseMirror builtins.
    const expected = new Set<string>(NODE_KINDS);
    expected.add("doc");
    expected.add("text");
    for (const name of Object.keys(editor.schema.nodes)) {
      expect(expected.has(name), `unexpected PM node "${name}" — not in tools/schema.json`).toBe(true);
    }
    editor.destroy();
  });

  it("every Tiptap mark maps to a snake_case kind from tools/schema.json", () => {
    const doc = new Y.Doc();
    const awareness = new Awareness(doc);
    const editor = new Editor({
      extensions: createExtensions({
        doc,
        awareness,
        user: { name: "test", color: "#000" },
      }),
    });
    for (const kind of MARK_KINDS) {
      expect(editor.schema.marks[kind], `mark ${kind} missing from PM schema`).toBeDefined();
    }
    const expected = new Set<string>(MARK_KINDS);
    for (const name of Object.keys(editor.schema.marks)) {
      expect(expected.has(name), `unexpected PM mark "${name}" — not in tools/schema.json`).toBe(true);
    }
    editor.destroy();
  });
});
