import { describe, it, expect, vi, beforeEach } from "vitest";
import { apiFetch } from "./api";

beforeEach(() => {
  vi.restoreAllMocks();
  document.cookie = "csrf=test-token; Path=/";
});

describe("apiFetch", () => {
  it("returns ok for 200 JSON", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ id: "x" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    const r = await apiFetch<{ id: string }>("/api/foo");
    if ("ok" in r) expect(r.ok.id).toBe("x");
    else throw new Error("expected ok");
  });

  it("returns error envelope for 4xx", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ error: { code: "auth.csrf", message: "no", details: {} } }), {
        status: 403,
        headers: { "Content-Type": "application/json" },
      }),
    );
    const r = await apiFetch("/api/foo", { method: "POST", body: {} });
    if ("error" in r) {
      expect(r.error.code).toBe("auth.csrf");
      expect(r.error.status).toBe(403);
    } else throw new Error("expected error");
  });

  it("sends X-CSRF-Token on POST", async () => {
    const spy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(null, { status: 200 }),
    );
    await apiFetch("/api/foo", { method: "POST", body: { a: 1 } });
    const init = spy.mock.calls[0]?.[1];
    const headers = init?.headers as Record<string, string> | undefined;
    expect(headers?.["X-CSRF-Token"]).toBe("test-token");
    expect(headers?.["Content-Type"]).toBe("application/json");
  });

  it("does NOT send X-CSRF-Token on GET", async () => {
    const spy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response("{}", { status: 200, headers: { "Content-Type": "application/json" } }),
    );
    await apiFetch("/api/foo");
    const init = spy.mock.calls[0]?.[1];
    const headers = init?.headers as Record<string, string> | undefined;
    expect(headers?.["X-CSRF-Token"]).toBeUndefined();
  });
});
