// Prereq: clean Postgres + knot-server pointed at it. The Playwright
// `webServer` config boots the server; this spec's beforeAll truncates
// auth tables so the run is repeatable.

import { test, expect, request } from "@playwright/test";
import { execSync } from "node:child_process";

const SERVER = "http://localhost:3000";

function resetAuthTables() {
  // The dev compose stack must be up (`make compose.up`). This truncates
  // the auth tables so `/auth/setup` can succeed afresh.
  const cmd = [
    "docker compose",
    "-f deploy/compose/dev.yml",
    "exec -T postgres",
    `psql -U knot -d knot -c`,
    `"TRUNCATE TABLE acl_invalidations, audit_events, doc_markdown_cache, doc_snapshots, doc_updates, document_grants, documents, sessions, workspace_members, users, workspaces CASCADE"`,
  ].join(" ");
  execSync(cmd, { cwd: "..", stdio: "pipe" });
}

test.beforeAll(() => {
  resetAuthTables();
});

test("local auth round-trip: setup → session → logout", async () => {
  const ctx = await request.newContext({ baseURL: SERVER });

  const setup = await ctx.post("/auth/setup", {
    data: {
      email: "e2e-admin@example.com",
      password: "e2e-hunter22",
      display_name: "E2E Admin",
    },
  });
  expect(setup.status()).toBe(201);
  const setCookie = setup.headers()["set-cookie"];
  expect(setCookie).toContain("sid=");

  const session = await ctx.get("/auth/session");
  expect(session.status()).toBe(200);
  const body = await session.json();
  expect(body.email).toBe("e2e-admin@example.com");
  expect(body.role).toBe("owner");

  const logout = await ctx.post("/auth/logout");
  expect(logout.status()).toBe(204);

  const after = await ctx.get("/auth/session");
  expect(after.status()).toBe(401);
});

test("login wrong password returns 401", async () => {
  const ctx = await request.newContext({ baseURL: SERVER });
  const r = await ctx.post("/auth/login", {
    data: { email: "does-not-exist@example.com", password: "wrong" },
  });
  expect(r.status()).toBe(401);
});
