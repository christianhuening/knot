import { type ApiError, type ApiResult, apiFetch } from "./api";
import { readCookie } from "./csrf";

export type Board = {
  id: string;
  doc_id: string;
  created_by: string;
  label: string | null;
  svg_seq: number;
  created_at: string;
  updated_at: string;
};

export const boardsApi = {
  async create(docId: string, label: string | null): Promise<ApiResult<Board>> {
    return apiFetch<Board>(`/api/docs/${encodeURIComponent(docId)}/boards`, {
      method: "POST",
      body: { label },
    });
  },

  async list(docId: string): Promise<ApiResult<Board[]>> {
    return apiFetch<Board[]>(`/api/docs/${encodeURIComponent(docId)}/boards`);
  },

  // SVG endpoints carry image/svg+xml bodies; apiFetch is JSON-shaped so we
  // talk to fetch directly here (mirrors blobs.api.ts for the binary path).
  async getSvg(boardId: string): Promise<ApiResult<string>> {
    let res: Response;
    try {
      res = await fetch(`/api/boards/${encodeURIComponent(boardId)}/svg`, {
        method: "GET",
        credentials: "include",
        headers: { Accept: "image/svg+xml" },
      });
    } catch {
      return { error: { code: "network", message: "Network error", details: {}, status: 0 } };
    }
    const text = await res.text();
    if (!res.ok) {
      try {
        const env = JSON.parse(text) as { error?: Partial<ApiError> };
        return {
          error: {
            code: env.error?.code ?? "http_error",
            message: env.error?.message ?? `HTTP ${res.status}`,
            details: env.error?.details ?? {},
            status: res.status,
          },
        };
      } catch {
        return { error: { code: "http_error", message: `HTTP ${res.status}`, details: {}, status: res.status } };
      }
    }
    return { ok: text };
  },

  async putSvg(boardId: string, svg: string): Promise<ApiResult<void>> {
    const headers: Record<string, string> = { "Content-Type": "image/svg+xml" };
    const csrf = readCookie("csrf");
    if (csrf) headers["X-CSRF-Token"] = csrf;
    let res: Response;
    try {
      res = await fetch(`/api/boards/${encodeURIComponent(boardId)}/svg`, {
        method: "PUT",
        credentials: "include",
        headers,
        body: svg,
      });
    } catch {
      return { error: { code: "network", message: "Network error", details: {}, status: 0 } };
    }
    if (!res.ok) {
      const text = await res.text();
      try {
        const env = JSON.parse(text) as { error?: Partial<ApiError> };
        return {
          error: {
            code: env.error?.code ?? "http_error",
            message: env.error?.message ?? `HTTP ${res.status}`,
            details: env.error?.details ?? {},
            status: res.status,
          },
        };
      } catch {
        return { error: { code: "http_error", message: `HTTP ${res.status}`, details: {}, status: res.status } };
      }
    }
    return { ok: undefined };
  },
};
