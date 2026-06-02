import { EditorContent, useEditor } from "@tiptap/react";
import { useEffect, useMemo } from "react";
import * as Y from "yjs";

import { useSession } from "../../auth/SessionContext";

import { createExtensions } from "./extensions";
import { KnotProvider, type ProviderStatus } from "./KnotProvider";

export function KnotEditor({
  docId,
  onStatus,
  role,
}: {
  docId: string;
  onStatus: (s: ProviderStatus) => void;
  role: "owner" | "editor" | "viewer";
}) {
  const session = useSession();
  const sessionUser = session.data && "ok" in session.data ? session.data.ok : null;
  const ydoc = useMemo(() => new Y.Doc(), [docId]);

  const provider = useMemo(() => {
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    return new KnotProvider({
      url: `${proto}//${window.location.host}/collab/${docId}`,
      doc: ydoc,
    });
  }, [ydoc, docId]);

  useEffect(() => {
    onStatus(provider.status);
    const fn = (s: ProviderStatus) => onStatus(s);
    provider.on("status", fn);
    return () => {
      provider.off("status", fn);
      provider.destroy();
      ydoc.destroy();
    };
  }, [provider, ydoc, onStatus]);

  const userColor = useMemo(() => colorFor(sessionUser?.user_id ?? "anon"), [sessionUser]);

  const editor = useEditor(
    {
      extensions: createExtensions({
        doc: ydoc,
        awareness: provider.awareness,
        user: { name: sessionUser?.display_name ?? "Anonymous", color: userColor },
      }),
      editable: role !== "viewer",
    },
    [ydoc, provider, sessionUser?.user_id, role],
  );

  return (
    <div data-testid="editor-host" style={{ border: "1px solid #e5e5e5", padding: 16, minHeight: 240 }}>
      <EditorContent editor={editor} />
    </div>
  );
}

function colorFor(id: string): string {
  let hash = 0;
  for (let i = 0; i < id.length; i += 1) hash = (hash * 31 + id.charCodeAt(i)) >>> 0;
  return `hsl(${hash % 360}, 70%, 45%)`;
}
