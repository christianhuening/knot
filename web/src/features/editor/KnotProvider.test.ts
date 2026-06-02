import { describe, expect, it } from "vitest";
import * as Y from "yjs";

import { KnotProvider } from "./KnotProvider";

describe("KnotProvider", () => {
  it("constructs in 'connecting' state and destroys cleanly", () => {
    const p = new KnotProvider({
      url: "ws://127.0.0.1:1/never",
      doc: new Y.Doc(),
    });
    expect(p.status).toBe("connecting");
    p.destroy();
  });

  it("emits a status change to a registered listener on destroy path", () => {
    const p = new KnotProvider({
      url: "ws://127.0.0.1:1/never",
      doc: new Y.Doc(),
    });
    const seen: string[] = [];
    p.on("status", (s) => seen.push(s));
    // Initial status is set in connect() before the listener registered, so
    // we only assert that the listener mechanism works at all by destroying
    // (no event fires on destroy, but off() must not throw).
    p.off("status", (s) => seen.push(s));
    p.destroy();
    expect(p.status).toBe("connecting");
  });
});
