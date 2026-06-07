import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { mkdirSync } from "node:fs";

const SHOTS = "ux/screenshots";

test.beforeAll(() => {
  mkdirSync(SHOTS, { recursive: true });
});

// One spec per app state; the placeholder app currently has the scan screen.
// As views are built, add states (empty / loading / results / confirm / error).
test("scan screen", async ({ page }, testInfo) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "mac-cleaner" })).toBeVisible();

  // Artifact the ux-critic (and human) review — named per viewport project.
  await page.screenshot({
    path: `${SHOTS}/scan-${testInfo.project.name}.png`,
    fullPage: true,
  });

  // Objective gate: no serious/critical accessibility violations.
  const results = await new AxeBuilder({ page }).analyze();
  const blocking = results.violations.filter(
    (v) => v.impact === "serious" || v.impact === "critical",
  );
  expect(blocking, JSON.stringify(blocking, null, 2)).toEqual([]);

  // Objective gate: visual regression (intentional changes only).
  await expect(page).toHaveScreenshot(`scan-${testInfo.project.name}.png`, {
    fullPage: true,
  });
});
