import * as v from "valibot";

import { apiFetch } from "../../lib/api";
import { Grant, parse } from "../../lib/validators";

export const grantsApi = {
  async list(docId: string) {
    const r = await apiFetch<unknown>(
      `/api/docs/${encodeURIComponent(docId)}/grants`,
    );
    if ("error" in r) return r;
    return { ok: parse(v.array(Grant), r.ok) };
  },
  put(docId: string, principal: string, role: "owner" | "editor" | "viewer", inherit: boolean) {
    return apiFetch<void>(
      `/api/docs/${encodeURIComponent(docId)}/grants/${encodeURIComponent(principal)}`,
      { method: "PUT", body: { role, inherit } },
    );
  },
  remove(docId: string, principal: string) {
    return apiFetch<void>(
      `/api/docs/${encodeURIComponent(docId)}/grants/${encodeURIComponent(principal)}`,
      { method: "DELETE" },
    );
  },
};
