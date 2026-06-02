import { readCookie } from "./csrf";

export type ApiError = {
  code: string;
  message: string;
  details: Record<string, unknown>;
  status: number;
};

export type ApiResult<T> = { ok: T } | { error: ApiError };

type Body = Record<string, unknown> | string | null;

type Opts = {
  method?: "GET" | "POST" | "PATCH" | "PUT" | "DELETE";
  body?: Body;
  contentType?: string;
  parser?: (data: unknown) => unknown;
};

const UNSAFE = new Set(["POST", "PUT", "PATCH", "DELETE"]);

export async function apiFetch<T>(path: string, opts: Opts = {}): Promise<ApiResult<T>> {
  const method = opts.method ?? "GET";
  const headers: Record<string, string> = { Accept: "application/json" };
  let body: BodyInit | undefined = undefined;
  if (opts.body !== undefined && opts.body !== null) {
    if (typeof opts.body === "string") {
      body = opts.body;
      headers["Content-Type"] = opts.contentType ?? "text/plain";
    } else {
      body = JSON.stringify(opts.body);
      headers["Content-Type"] = "application/json";
    }
  }
  if (UNSAFE.has(method)) {
    const csrf = readCookie("csrf");
    if (csrf) headers["X-CSRF-Token"] = csrf;
  }
  const res = await fetch(path, { method, credentials: "include", headers, body });
  const text = await res.text();
  let data: unknown = undefined;
  if (text.length > 0) {
    if (res.headers.get("content-type")?.includes("application/json")) {
      try { data = JSON.parse(text); } catch { data = text; }
    } else {
      data = text;
    }
  }
  if (!res.ok) {
    const env = data as { error?: Partial<ApiError> } | undefined;
    return {
      error: {
        code: env?.error?.code ?? "http_error",
        message: env?.error?.message ?? `HTTP ${res.status}`,
        details: env?.error?.details ?? {},
        status: res.status,
      },
    };
  }
  const parsed = opts.parser ? opts.parser(data) : data;
  return { ok: parsed as T };
}
