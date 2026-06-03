import { execSync } from "node:child_process";
import { expect, test } from "@playwright/test";

function reset() {
  const tables = [
    "acl_invalidations","audit_events","doc_markdown_cache","doc_snapshots","doc_updates",
    "document_grants","documents","sessions","workspace_members","users","workspaces",
    "blobs","blob_bytes",
  ].join(", ");
  execSync(
    `docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c "TRUNCATE TABLE ${tables} CASCADE"`,
    { cwd: "..", stdio: "pipe" },
  );
}
test.beforeAll(reset);
test.use({ viewport: { width: 375, height: 667 } });

test("mobile: drawer opens/closes, palette goes full-screen", async ({ page }) => {
  await page.goto("/setup");
  await page.getByTestId("setup-email").fill("o@e.com");
  await page.getByTestId("setup-display-name").fill("O");
  await page.getByTestId("setup-password").fill("owner-hunter22");
  await page.getByTestId("setup-submit").click();
  await page.waitForURL(/\/(?:doc\/.+)?$/);

  // Initial state: sidebarOpen=true (Zustand default), so backdrop is showing.
  const backdrop = page.getByTestId("sidebar-backdrop");
  await expect(backdrop).toBeVisible();
  // Close drawer by tapping the backdrop's right-hand region (the sidebar
  // sits over the left 260 px and intercepts clicks there).
  await backdrop.click({ position: { x: 350, y: 300 } });
  await expect(backdrop).toHaveCount(0);

  // Hamburger toggle now visible.
  const toggle = page.getByTestId("menu-toggle");
  await expect(toggle).toBeVisible();

  // Open drawer via toggle.
  await toggle.click();
  await expect(backdrop).toBeVisible();
  await expect(page.getByTestId("sidebar")).toBeVisible();

  // Close again to free input focus before opening palette.
  await backdrop.click({ position: { x: 350, y: 300 } });
  await expect(backdrop).toHaveCount(0);

  // Open palette — should cover viewport width.
  await page.keyboard.press("Control+k");
  const dialog = page.getByTestId("cmdk");
  await expect(dialog).toBeVisible();
  const box = await dialog.boundingBox();
  expect(box?.width).toBeGreaterThan(370);
  expect(box?.height).toBeGreaterThan(660);
});
