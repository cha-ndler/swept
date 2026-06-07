import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { mkdirSync } from "node:fs";

const SHOTS = "ux/screenshots";

test.beforeAll(() => {
  mkdirSync(SHOTS, { recursive: true });
});

// Deterministic states via the ?state= preview override (see src/App.tsx).
const STATES = ["results", "empty", "loading", "confirm", "done"] as const;

for (const state of STATES) {
  test(`scan ${state}`, async ({ page }, testInfo) => {
    await page.goto(`/?state=${state}`);
    await expect(page.getByRole("heading", { name: "mac-cleaner" })).toBeVisible();

    // Artifact for the ux-critic / human review (named per state × viewport).
    await page.screenshot({
      path: `${SHOTS}/scan-${state}-${testInfo.project.name}.png`,
      fullPage: true,
    });

    // Objective gate: no serious/critical accessibility violations.
    const results = await new AxeBuilder({ page }).analyze();
    const blocking = results.violations.filter(
      (v) => v.impact === "serious" || v.impact === "critical",
    );
    expect(blocking, JSON.stringify(blocking, null, 2)).toEqual([]);

    // Objective gate: visual regression (intentional changes only).
    await expect(page).toHaveScreenshot(`scan-${state}.png`, { fullPage: true });
  });
}
