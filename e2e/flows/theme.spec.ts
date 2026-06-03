import { execSync } from "node:child_process";

import { expect, test } from "@playwright/test";

function reset() {
  const tables = [
    "comment_reactions", "comments",
    "acl_invalidations", "audit_events", "doc_markdown_cache",
    "doc_snapshots", "doc_updates", "document_grants", "documents",
    "sessions", "workspace_members", "users", "workspaces",
  ].join(", ");
  execSync(
    `docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c "TRUNCATE TABLE ${tables} CASCADE"`,
    { cwd: "..", stdio: "pipe" },
  );
}

test.beforeAll(reset);

test("theme toggle persists across reload", async ({ page }) => {
  // Setup owner so the workspace sidebar (which hosts the toggle) renders.
  await page.goto("/setup");
  await page.getByTestId("setup-email").fill("owner@theme.test");
  await page.getByTestId("setup-display-name").fill("Owner");
  await page.getByTestId("setup-password").fill("hunter22!theme");
  await page.getByTestId("setup-submit").click();
  await page.waitForURL(/\/(?:doc\/.+)?$/);

  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");

  const toggle = page.getByTestId("theme-toggle");
  await expect(toggle).toBeVisible();
  await toggle.click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  await page.getByTestId("theme-toggle").click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
});
