import { test, expect } from "@playwright/test";
import type { Page } from "@playwright/test";
import type { ScanReport } from "../src/types";

// Regression gate for the trust bug: when the Rust backend returns an error,
// the app must SAY SO. It must never silently substitute fixture data — the
// category ids in the fixtures are the real ones, so a user could otherwise
// act on fabricated sizes and dispose of real files.
//
// The seam is Tauri's own transport: `@tauri-apps/api`'s `invoke` delegates to
// `window.__TAURI_INTERNALS__.invoke` (node_modules/@tauri-apps/api/core.js:202),
// so stubbing that object simulates "running inside the app" precisely.
async function stubBackend(page: Page, behavior: string) {
  await page.addInitScript(`
    window.__TAURI_INTERNALS__ = {
      invoke: (cmd) => {
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        return ${behavior};
      },
      transformCallback: (cb) => cb,
    };
  `);
}

test("a failing scan shows the error state, not fixture data", async ({
  page,
}) => {
  await stubBackend(
    page,
    `Promise.reject("permission denied reading ~/Library/Caches")`,
  );
  await page.goto("/");

  await expect(page.getByText(/couldn.t finish/i)).toBeVisible();
  await expect(page.getByText(/permission denied/i)).toBeVisible();

  // The tells that fixture data leaked into a real session.
  await expect(page.getByText("6.4 GiB")).toHaveCount(0);
  await expect(page.getByText("Xcode derived data")).toHaveCount(0);
});

test("a failing login-items call shows an error, not fixture items", async ({
  page,
}) => {
  await stubBackend(page, `Promise.reject("permission denied")`);
  await page.goto("/?tab=startup");

  await expect(page.getByText("com.docker.helper")).toHaveCount(0);
  await expect(page.getByText("com.spotify.webhelper")).toHaveCount(0);
});

// A build-artifact fact rather than a behavioural one, so it cannot rot the way
// a mocked test can: if fixtures are ever imported from `src/` again, the
// strings land in `dist/` and this fails regardless of how the UI behaves.
test("the shipped bundle contains no fixture data", async () => {
  const { readdirSync, readFileSync } = await import("node:fs");
  const { join } = await import("node:path");

  const dir = join(process.cwd(), "dist", "assets");
  const bundles = readdirSync(dir).filter((f) => f.endsWith(".js"));
  expect(bundles.length).toBeGreaterThan(0);

  // Tells must be strings that ONLY a fixture could supply. Category *ids* do
  // not qualify: the frontend legitimately knows them (CleanView maps each id
  // to its category hue), so "xcode-derived-data" appearing in the bundle
  // proves nothing. Human-readable names, descriptions and counts do qualify —
  // they are rendered straight from the backend response and are never
  // authored in src/.
  const tells = [
    "com.docker.helper",
    "com.spotify.webhelper",
    "4213",
    "Build intermediates and indexes",
    "Cached package downloads",
    "Per-user app caches",
    "Xcode derived data",
  ];
  for (const file of bundles) {
    const source = readFileSync(join(dir, file), "utf8");
    for (const tell of tells) {
      expect(source, `${file} leaks fixture data: ${tell}`).not.toContain(tell);
    }
  }
});

// The plain-browser path (`npm run dev`, or anyone opening dist/ directly).
// There is no backend to ask, so the app must say that rather than invent a
// disk. This is the state the old code hid behind fixture data.
test("outside the desktop app it says so instead of showing a disk", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByText(/runs as a desktop app/i)).toBeVisible();
  await expect(page.getByText("6.4 GiB")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /review & clean/i }),
  ).toHaveCount(0);
});

// A confirmation sheet must never outlive the report it describes.
//
// Chain (demonstrated by the safety reviewer, introduced by the fixture-removal
// change): scan succeeds -> user opens the sheet -> a filter change triggers a
// re-scan -> the re-scan fails -> the failure handler cleared the selection but
// left the sheet open, so it read "Move 0 items" while an empty selection was
// mapped backend-side to "no filter", i.e. every category. Confirming it would
// have run an unrestricted clean the user had confirmed as zero items.
test("a failed re-scan closes the confirmation instead of emptying it", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    let scans = 0;
    (w as Record<string, unknown>).__cleanCalls = [];
    w.__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        if (cmd === "scan") {
          scans += 1;
          if (scans > 1) return Promise.reject("permission denied");
          // `satisfies`, so the compiler keeps this mock honest: an untyped
          // literal silently kept modelling a payload the backend had stopped
          // sending, which is exactly how a screen gets tested against a shape
          // that no longer exists.
          return Promise.resolve({
            total_count: 120,
            total_bytes: 1024,
            requires_confirmation: true,
            skipped_protected: 0,
            skipped_unreadable: 0,
            partial: false,
            items: [],
            by_category: [
              {
                category: "user-logs",
                name: "Logs",
                description: "d",
                count: 120,
                bytes: 1024,
                smart_scan_default: true,
              },
            ],
          } satisfies ScanReport);
        }
        if (cmd === "clean") {
          ((w as Record<string, unknown>).__cleanCalls as unknown[]).push(args);
          return Promise.resolve({
            dry_run: false,
            executed: 0,
            refused: 0,
            bytes_freed: 0,
          });
        }
        return Promise.resolve([]);
      },
      transformCallback: (cb: unknown) => cb,
    };
  });

  await page.goto("/");
  await page.getByRole("button", { name: /review & clean/i }).click();
  await expect(page.getByRole("dialog")).toBeVisible();

  // Trigger a re-scan while the sheet is open. The event is dispatched straight
  // at the control because the overlay deliberately blocks pointer input — and
  // the invariant under test is not "the overlay is permeable", it is that
  // `runScan` closes the sheet no matter what triggered it. (The old `<select>`
  // hid this: selectOption sets the value through the DOM, so it never had to
  // get past the overlay either.)
  await page.getByRole("radio", { name: "100 MB" }).dispatchEvent("click");

  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByText(/couldn.t finish/i)).toBeVisible();
  expect(
    await page.evaluate(
      () => (window as never as Record<string, unknown[]>).__cleanCalls,
    ),
  ).toEqual([]);
});
