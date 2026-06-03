import type { Doc } from "../../lib/validators";

export type TreeNode = Doc & { children: TreeNode[] };

/** Build a tree from a flat doc list. Sorts siblings by sort_key
 *  (LexoRank-style, lexicographic). Orphans (parent_id missing) become
 *  top-level. */
export function buildTree(docs: Doc[]): TreeNode[] {
  const byId = new Map<string, TreeNode>();
  docs.forEach((d) => byId.set(d.id, { ...d, children: [] }));
  const roots: TreeNode[] = [];
  byId.forEach((node) => {
    if (node.parent_id && byId.has(node.parent_id)) {
      byId.get(node.parent_id)!.children.push(node);
    } else {
      roots.push(node);
    }
  });
  const sortKey = (a: TreeNode, b: TreeNode) =>
    a.sort_key < b.sort_key ? -1 : a.sort_key > b.sort_key ? 1 : 0;
  function sortRec(nodes: TreeNode[]) {
    nodes.sort(sortKey);
    nodes.forEach((n) => sortRec(n.children));
  }
  sortRec(roots);
  return roots;
}

/** Given a flat doc list, produce a flat list with one doc moved to a new
 *  parent (drop-onto-row). Used for optimistic UI before the server confirms. */
export function reorderInto(docs: Doc[], movedId: string, newParentId: string | null): Doc[] {
  return docs.map((d) => (d.id === movedId ? { ...d, parent_id: newParentId } : d));
}

/** Map a drop target + drop position to the args expected by
 *  POST /api/docs/:id/move. */
export function moveArgs(
  target: Doc | null,
  position: "before" | "after" | "into",
): { parent_id?: string | null; before_id?: string; after_id?: string } {
  if (!target) return { parent_id: null };
  switch (position) {
    case "before": return { parent_id: target.parent_id, before_id: target.id };
    case "after":  return { parent_id: target.parent_id, after_id: target.id };
    case "into":   return { parent_id: target.id };
  }
}
