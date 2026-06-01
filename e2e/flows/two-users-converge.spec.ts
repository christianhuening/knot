import { test, expect } from "@playwright/test";

test("two users editing concurrently converge on both screens", async ({ browser }) => {
  const docId = `t-${Date.now()}`;

  const ctxA = await browser.newContext();
  const ctxB = await browser.newContext();
  const pageA = await ctxA.newPage();
  const pageB = await ctxB.newPage();

  await pageA.goto(`/?docId=${docId}`);
  await pageB.goto(`/?docId=${docId}`);

  // Wait for both editors to reach "connected" status.
  await expect(pageA.getByTestId("editor-status")).toHaveText("connected", { timeout: 30_000 });
  await expect(pageB.getByTestId("editor-status")).toHaveText("connected", { timeout: 30_000 });

  const editorA = pageA.getByTestId("editor-host").locator(".ProseMirror");
  const editorB = pageB.getByTestId("editor-host").locator(".ProseMirror");

  await editorA.click();
  await editorA.type("Hello from Alice. ");

  await editorB.click();
  await editorB.type("And from Bob.");

  // Both screens see both contributions within the poll window.
  await expect.poll(() => editorA.textContent(), { timeout: 5_000 }).toMatch(/Hello from Alice\./);
  await expect.poll(() => editorA.textContent(), { timeout: 5_000 }).toMatch(/And from Bob\./);
  await expect.poll(() => editorB.textContent(), { timeout: 5_000 }).toMatch(/Hello from Alice\./);
  await expect.poll(() => editorB.textContent(), { timeout: 5_000 }).toMatch(/And from Bob\./);

  await ctxA.close();
  await ctxB.close();
});
