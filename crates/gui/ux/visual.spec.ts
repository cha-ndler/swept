import { test, expect } from "@playwright/test";
import type { Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { mkdirSync } from "node:fs";
import { SAMPLE_LOGIN_ITEMS, SAMPLE_REPORT, SAMPLE_SUMMARY } from "./fixtures";

const SHOTS = "ux/screenshots";

test.beforeAll(() => {
  mkdirSync(SHOTS, { recursive: true });
});

/**
 * Install a fake Tauri host before the page loads.
 *
 * The app has exactly one data path — `invoke` — and no in-app fixtures, so the
 * oracle has to supply the backend rather than ask the app to pretend. We stub
 * `window.__TAURI_INTERNALS__`, which is what `@tauri-apps/api`'s `invoke`
 * delegates to (node_modules/@tauri-apps/api/core.js:202). That makes these
 * renders exercise the same code path the real app runs, instead of a preview
 * branch that only existed for the screenshots.
 */
async function installBackend(
  page: Page,
  opts: {
    report?: unknown;
    items?: unknown;
    summary?: unknown;
    hang?: boolean;
    perms?: unknown;
  } = {},
) {
  const payload = {
    report: opts.report ?? SAMPLE_REPORT,
    items: opts.items ?? SAMPLE_LOGIN_ITEMS,
    summary: opts.summary ?? SAMPLE_SUMMARY,
    hang: opts.hang ?? false,
    // Full access unless a test says otherwise, so the notice stays out of the
    // other screenshots.
    perms: opts.perms ?? {
      trash_readable: true,
      containers_readable: true,
      all_readable: true,
    },
  };
  await page.addInitScript((p) => {
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: { handler?: (e: unknown) => void }) => {
        // `listen()` round-trips through invoke; `transformCallback` below is the
        // identity, so `args.handler` is the raw callback. Stash it so a test can
        // drive real progress renders.
        if (cmd === "plugin:event|listen") {
          w.__emit = (payload: unknown) => args.handler?.({ payload });
          return Promise.resolve(1);
        }
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        // The permission probe is advisory and must answer even while a scan
        // hangs, otherwise the loading screenshot would race it.
        if (cmd === "permissions") return Promise.resolve(p.perms);
        if (p.hang) return new Promise(() => {});
        if (cmd === "scan") return Promise.resolve(p.report);
        if (cmd === "login_items") return Promise.resolve(p.items);
        if (cmd === "clean") return Promise.resolve(p.summary);
        return Promise.reject(new Error(`unstubbed command: ${cmd}`));
      },
      transformCallback: (cb: unknown) => cb,
    };
  }, payload);
}

async function capture(page: Page, name: string, project: string) {
  await page.screenshot({ path: `${SHOTS}/${name}-${project}.png`, fullPage: true });

  // Objective gate: no serious/critical accessibility violations.
  const results = await new AxeBuilder({ page }).analyze();
  const blocking = results.violations.filter(
    (v) => v.impact === "serious" || v.impact === "critical",
  );
  expect(blocking, JSON.stringify(blocking, null, 2)).toEqual([]);

  // Objective gate: visual regression (intentional changes only).
  await expect(page).toHaveScreenshot(`${name}.png`, { fullPage: true });
}

test("scan results", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Cleanup" })).toBeVisible();
  await capture(page, "scan-results", testInfo.project.name);
});

test("scan empty", async ({ page }, testInfo) => {
  await installBackend(page, {
    report: { ...SAMPLE_REPORT, total_count: 0, total_bytes: 0, by_category: [] },
  });
  await page.goto("/");
  await expect(page.getByText("Nothing to clean")).toBeVisible();
  await capture(page, "scan-empty", testInfo.project.name);
});

test("scan loading", async ({ page }, testInfo) => {
  await installBackend(page, { hang: true });
  await page.goto("/");
  await expect(page.getByRole("status")).toBeVisible();

  // Drive a real progress reading so the snapshot shows the state a user
  // actually sees mid-scan, not just the initial frame.
  await page.evaluate(() => {
    const w = window as unknown as { __emit?: (p: unknown) => void };
    w.__emit?.({ examined: 48231, planned: 46012, bytes: 7_600_000_000 });
  });
  await expect(page.getByText(/48,231 files examined/)).toBeVisible();

  await capture(page, "scan-loading", testInfo.project.name);
});

test("scan results with limited access", async ({ page }, testInfo) => {
  await installBackend(page, {
    perms: { trash_readable: false, containers_readable: true, all_readable: false },
  });
  await page.goto("/");
  await expect(page.getByText(/under-reporting/i)).toBeVisible();
  // The figures shown are still real — the notice explains a gap, it does not
  // replace or qualify the numbers themselves.
  await expect(page.getByText("6.4 GiB")).toBeVisible();
  await capture(page, "scan-limited-access", testInfo.project.name);
});

test("full access shows no notice", async ({ page }) => {
  await installBackend(page);
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Cleanup" })).toBeVisible();
  await expect(page.getByText(/under-reporting/i)).toHaveCount(0);
});

test("scan confirm", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/");
  await page.getByRole("button", { name: /review & clean/i }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await capture(page, "scan-confirm", testInfo.project.name);
});

test("scan done", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/");
  await page.getByRole("button", { name: /review & clean/i }).click();
  await page.getByRole("button", { name: /^move to/i }).click();
  await expect(page.getByText(/moved to the Trash/i)).toBeVisible();
  await expect(page.getByRole("button", { name: /back to cleanup/i })).toBeVisible();
  await capture(page, "scan-done", testInfo.project.name);
});

test("scan error", async ({ page }, testInfo) => {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = {
      invoke: (cmd: string) => {
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        return Promise.reject("Couldn’t read ~/Library/Caches (permission denied).");
      },
      transformCallback: (cb: unknown) => cb,
    };
  });
  await page.goto("/");
  await expect(page.getByText(/couldn.t finish/i)).toBeVisible();
  await capture(page, "scan-error", testInfo.project.name);
});

test("startup tab", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/?tab=startup");
  await expect(page.getByRole("heading", { name: "Startup" })).toBeVisible();
  await capture(page, "startup", testInfo.project.name);
});
