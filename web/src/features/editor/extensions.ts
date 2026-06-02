import Collaboration from "@tiptap/extension-collaboration";
import CollaborationCursor from "@tiptap/extension-collaboration-cursor";
import StarterKit from "@tiptap/starter-kit";
import type { Awareness } from "y-protocols/awareness";
import type * as Y from "yjs";

/** Canonical Tiptap extension set that matches the server schema generated
 *  from `tools/schema.json`. History is disabled because Yjs UndoManager
 *  owns undo. */
export function createExtensions(opts: {
  doc: Y.Doc;
  awareness: Awareness;
  user: { name: string; color: string };
}) {
  return [
    StarterKit.configure({
      history: false,
    }),
    Collaboration.configure({ document: opts.doc }),
    CollaborationCursor.configure({
      provider: { awareness: opts.awareness } as never,
      user: opts.user,
    }),
  ];
}
