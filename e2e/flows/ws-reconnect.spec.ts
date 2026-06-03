import { execSync } from "node:child_process";
import { expect, test } from "@playwright/test";

function reset() {
  const tables = ["acl_invalidations","audit_events","doc_markdown_cache","doc_snapshots","doc_updates","document_grants","documents","sessions","workspace_members","users","workspaces"].join(", ");
  execSync(`docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c "TRUNCATE TABLE ${tables} CASCADE"`, { cwd: "..", stdio: "pipe" });
}
test.beforeAll(reset);

// SKIPPED: Playwright's context.setOffline(true) blocks NEW connections but
// doesn't synchronously close the existing WebSocket — Chromium dispatches the
// offline transition via TCP keepalive timeout (30s+ on Linux), so the
// status-dot stays "connected" well past any reasonable test timeout.
//
// Alternatives we ruled out for v0.1:
//   - page.route() doesn't intercept WS frames.
//   - Exposing KnotProvider on window for ws.close() is invasive for tests.
//   - SIGKILL on the server kills the whole suite.
//
// The KnotProvider.scheduleReconnect path IS exercised by ungraceful close
// scenarios (server restart, NAT timeout in production); a follow-up should
// use a dedicated WS proxy/midfield to simulate the flap deterministically.
test.skip("editor reconnects after network flap; content preserved", async ({ page, context }) => {
  await page.goto("/setup");
  await page.getByTestId("setup-email").fill("o@e.com");
  await page.getByTestId("setup-display-name").fill("O");
  await page.getByTestId("setup-password").fill("owner-hunter22");
  await page.getByTestId("setup-submit").click();
  await page.getByTestId("new-doc").click();
  await page.waitForURL(/\/doc\/.+/);
  await expect(page.getByTestId("status-dot")).toHaveAttribute("data-status", "connected", { timeout: 10_000 });

  // Type something.
  const editor = page.locator("[data-testid='editor-host'] .ProseMirror");
  await editor.click();
  await page.keyboard.type("Before the flap.");
  await page.waitForTimeout(200);

  // Drop the network. KnotProvider's onclose fires; status → offline.
  await context.setOffline(true);
  await expect(page.getByTestId("status-dot")).toHaveAttribute("data-status", "offline", { timeout: 15_000 });

  // Restore. KnotProvider.scheduleReconnect should reconnect (first attempt
  // ~500ms backoff + jitter).
  await context.setOffline(false);
  await expect(page.getByTestId("status-dot")).toHaveAttribute("data-status", "connected", { timeout: 20_000 });

  // Content persists across the round-trip (Y.Doc never lost state).
  await expect(editor).toContainText("Before the flap.");
});
