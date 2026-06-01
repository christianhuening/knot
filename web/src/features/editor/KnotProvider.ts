import { WebsocketProvider } from "y-websocket";
import * as Y from "yjs";

export function createKnotProvider(opts: {
  url: string;
  docId: string;
  doc: Y.Doc;
}): WebsocketProvider {
  return new WebsocketProvider(opts.url, opts.docId, opts.doc, { connect: true });
}
