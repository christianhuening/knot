import { apiFetch, type ApiResult } from "./api";

export type SearchHit = {
  doc_id: string;
  parent_id: string | null;
  title: string;
  snippet: string;
  rank: number;
};

type SearchResponse = { results: SearchHit[] };

export const searchApi = {
  async query(q: string, limit = 20): Promise<ApiResult<SearchHit[]>> {
    const params = new URLSearchParams({ q, limit: String(limit) });
    const r = await apiFetch<SearchResponse>(`/api/search?${params.toString()}`);
    if ("error" in r) return r;
    return { ok: r.ok.results };
  },
};
