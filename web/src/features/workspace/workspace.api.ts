import * as v from "valibot";

import { apiFetch } from "../../lib/api";
import { Member, Workspace, parse } from "../../lib/validators";

export const workspaceApi = {
  async get() {
    const r = await apiFetch<unknown>("/api/workspace");
    if ("error" in r) return r;
    return { ok: parse(Workspace, r.ok) };
  },
  async listMembers() {
    const r = await apiFetch<unknown>("/api/workspace/members");
    if ("error" in r) return r;
    return { ok: parse(v.array(Member), r.ok) };
  },
  invite(email: string, role: "owner" | "editor" | "viewer") {
    return apiFetch<void>("/api/workspace/members", {
      method: "POST",
      body: { email, role },
    });
  },
  setRole(userId: string, role: "owner" | "editor" | "viewer") {
    return apiFetch<void>(`/api/workspace/members/${encodeURIComponent(userId)}`, {
      method: "PATCH",
      body: { role },
    });
  },
  remove(userId: string) {
    return apiFetch<void>(`/api/workspace/members/${encodeURIComponent(userId)}`, {
      method: "DELETE",
    });
  },
};
