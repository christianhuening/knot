import { describe, expect, it } from "vitest";
import type { Doc } from "../../lib/validators";
import { buildTree, reorderInto, moveArgs } from "./tree";

function doc(id: string, parent: string | null, sort_key: string): Doc {
  return {
    id,
    workspace_id: "w",
    parent_id: parent,
    title: id,
    sort_key,
    icon: null,
    created_by: "u",
    archived: false,
    is_template: false,
  };
}

describe("buildTree", () => {
  it("returns empty for empty input", () => {
    expect(buildTree([])).toEqual([]);
  });

  it("groups children under parents", () => {
    const t = buildTree([
      doc("a", null, "m"),
      doc("b", "a", "m"),
      doc("c", "a", "n"),
    ]);
    expect(t).toHaveLength(1);
    expect(t[0]!.id).toBe("a");
    expect(t[0]!.children.map((n) => n.id)).toEqual(["b", "c"]);
  });

  it("sorts siblings by sort_key", () => {
    const t = buildTree([
      doc("a", null, "n"),
      doc("b", null, "m"),
      doc("c", null, "z"),
    ]);
    expect(t.map((n) => n.id)).toEqual(["b", "a", "c"]);
  });

  it("treats orphans (parent missing) as top-level", () => {
    const t = buildTree([
      doc("a", null, "m"),
      doc("b", "missing-parent", "m"),
    ]);
    expect(t.map((n) => n.id).sort()).toEqual(["a", "b"]);
  });
});

describe("reorderInto", () => {
  it("reparents a doc", () => {
    const docs = [doc("a", null, "m"), doc("b", null, "n")];
    const next = reorderInto(docs, "b", "a");
    expect(next.find((d) => d.id === "b")?.parent_id).toBe("a");
  });
});

describe("moveArgs", () => {
  it("returns parent_id null when no target", () => {
    expect(moveArgs(null, "after")).toEqual({ parent_id: null });
  });
  it("before puts before_id", () => {
    const t = doc("a", "p", "m");
    expect(moveArgs(t, "before")).toEqual({ parent_id: "p", before_id: "a" });
  });
  it("after puts after_id", () => {
    const t = doc("a", "p", "m");
    expect(moveArgs(t, "after")).toEqual({ parent_id: "p", after_id: "a" });
  });
  it("into puts parent_id only", () => {
    const t = doc("a", "p", "m");
    expect(moveArgs(t, "into")).toEqual({ parent_id: "a" });
  });
});
