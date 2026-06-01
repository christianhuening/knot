import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Collaboration from "@tiptap/extension-collaboration";
import { useEffect, useMemo, useState } from "react";
import * as Y from "yjs";
import type { WebsocketProvider } from "y-websocket";
import { createKnotProvider } from "./KnotProvider";

type Status = "connecting" | "connected" | "offline";

export function KnotEditor({ docId }: { docId: string }) {
  const ydoc = useMemo(() => new Y.Doc(), [docId]);
  const [, setProvider] = useState<WebsocketProvider | null>(null);
  const [status, setStatus] = useState<Status>("connecting");

  useEffect(() => {
    const p = createKnotProvider({
      url: `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/collab`,
      docId,
      doc: ydoc,
    });
    const onStatus = (e: { status: "connecting" | "connected" | "disconnected" }) => {
      setStatus(e.status === "disconnected" ? "offline" : (e.status as Status));
    };
    p.on("status", onStatus);
    setProvider(p);
    return () => {
      p.off("status", onStatus);
      p.destroy();
      ydoc.destroy();
    };
  }, [ydoc, docId]);

  const editor = useEditor(
    {
      extensions: [
        StarterKit.configure({ history: false }),
        Collaboration.configure({ document: ydoc }),
      ],
    },
    [ydoc],
  );

  return (
    <section data-testid="editor-section">
      <header data-testid="editor-status">{status}</header>
      <div
        data-testid="editor-host"
        style={{ border: "1px solid #ccc", padding: 12, minHeight: 200 }}
      >
        <EditorContent editor={editor} />
      </div>
    </section>
  );
}
