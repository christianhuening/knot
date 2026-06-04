/**
 * yBinding — bridges Excalidraw's scene state to a Y.Doc `elements` map.
 *
 * Strategy (Option A): one Y.Map keyed by element id. Each value is a
 * cloned ExcalidrawElement. Last-write-wins per id, scoped by Excalidraw's
 * monotonic `version` field.
 *
 * The suppress flag is required because `observeDeep` fires SYNCHRONOUSLY
 * inside `ydoc.transact`. If we did not set the flag before transact, our
 * own writes would echo back into Excalidraw mid-render and loop.
 */

import * as Y from "yjs";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";

export type ExcalidrawBinding = {
  onChange: (next: readonly ExcalidrawElement[]) => void;
  destroy: () => void;
};

export function bindExcalidraw(
  api: ExcalidrawImperativeAPI,
  ydoc: Y.Doc,
): ExcalidrawBinding {
  const elements = ydoc.getMap<ExcalidrawElement>("elements");
  let suppressOnChange = false;
  // Excalidraw fires `onChange([])` on mount before the user has interacted.
  // If we let that empty snapshot run the delete-missing loop while the
  // Y.Map already holds remote state, we'd wipe the board for every peer.
  // The modal also gates `bindExcalidraw` on provider sync, but we keep this
  // defense-in-depth: only accept an empty snapshot as authoritative once
  // we have seen a non-empty one (i.e. the user actually deleted everything).
  let lastSeenNonEmpty = false;

  // Y → Excalidraw (initial + remote updates).
  function pushToExcalidraw() {
    if (suppressOnChange) return;
    const arr = Array.from(elements.values());
    // Excalidraw orders by fractional index (`el.index`); passing as-is is
    // fine — the renderer sorts internally. Avoid pre-sorting here.
    // Excalidraw treats input as immutable; do not mutate.
    api.updateScene({ elements: arr });
  }
  elements.observeDeep(pushToExcalidraw);
  pushToExcalidraw();

  // Excalidraw → Y (last-write-wins per element id).
  function onChange(next: readonly ExcalidrawElement[]) {
    // Ignore the mount-time empty snapshot: Excalidraw fires `onChange([])`
    // before any user interaction. If the Y.Map already holds remote state
    // and we have not yet seen a non-empty snapshot from Excalidraw, this
    // can only be the mount echo — not a real "user deleted everything"
    // event (which transitions from non-empty → empty).
    if (next.length === 0 && elements.size > 0 && !lastSeenNonEmpty) {
      return;
    }
    if (next.length > 0) lastSeenNonEmpty = true;
    // CRITICAL: set BEFORE transact. observeDeep fires synchronously inside
    // the transact body. If we toggled the flag inside or after, the
    // observer would see `false` and push our own write back into Excalidraw.
    suppressOnChange = true;
    try {
      ydoc.transact(() => {
        const nextIds = new Set<string>();
        for (const el of next) {
          nextIds.add(el.id);
          const prev = elements.get(el.id);
          if (!prev || prev.version !== el.version) {
            elements.set(el.id, globalThis.structuredClone(el));
          }
        }
        for (const id of Array.from(elements.keys())) {
          if (!nextIds.has(id)) elements.delete(id);
        }
      });
    } finally {
      suppressOnChange = false;
    }
  }

  function destroy() {
    elements.unobserveDeep(pushToExcalidraw);
  }

  return { onChange, destroy };
}
