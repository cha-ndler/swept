import { test, expect } from "@playwright/test";
import type { Page } from "@playwright/test";
import type { ScanReport, SmartScanReport } from "../src/types";

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
        // The harness stands in for a user who has already accepted the
        // terms, the same way the perms fixture stands in for one who granted
        // full disk access — otherwise every screenshot below would be of the
        // first-run sheet. The gate itself is captured by its own test.
        if (cmd === "terms_status")
          return Promise.resolve({
            accepted: true,
            terms_version: "1.0",
            terms_digest: "",
            accepted_version: null,
          });
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
  await page.goto("/?tab=cleanup");

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
  // not qualify: the frontend legitimately knows them (`hues.ts` maps each id
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
        // The harness stands in for a user who has already accepted the
        // terms, the same way the perms fixture stands in for one who granted
        // full disk access — otherwise every screenshot below would be of the
        // first-run sheet. The gate itself is captured by its own test.
        if (cmd === "terms_status")
          return Promise.resolve({
            accepted: true,
            terms_version: "1.0",
            terms_digest: "",
            accepted_version: null,
          });
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

  await page.goto("/?tab=cleanup");
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

// The property three rounds of backend review were spent on, asserted from the
// side that can actually violate it: **the set the request names must be the
// set the report offered.**
//
// The backend refuses each of these independently, and that is the point — this
// gate is here so a UI change cannot start relying on those refusals. A screen
// that sends a category the gesture never offers is broken whether or not
// something downstream catches it.
test("the Smart Scan request names only what the report offered", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__dispatched = [];
    w.__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        // The harness stands in for a user who has already accepted the
        // terms, the same way the perms fixture stands in for one who granted
        // full disk access — otherwise every screenshot below would be of the
        // first-run sheet. The gate itself is captured by its own test.
        if (cmd === "terms_status")
          return Promise.resolve({
            accepted: true,
            terms_version: "1.0",
            terms_digest: "",
            accepted_version: null,
          });
        if (cmd === "smart_scan") {
          return Promise.resolve({
            scanned_at_ms: 1_757_000_000_000,
            selected: { bytes: 3072, from: ["cleanup", "privacy"], incomplete: [] },
            found: { bytes: 7168, from: ["cleanup", "privacy", "large-old"], incomplete: [] },
            cleanup: [
              {
                category: "user-logs",
                smart_scan_default: true,
                name: "Logs",
                description: "d",
                count: 12,
                bytes: 2048,
              },
              // On the report, never on the gesture. Ticking it would empty the
              // Trash — the way back from every other row in the same click.
              {
                category: "trash",
                smart_scan_default: false,
                name: "Trash",
                description: "d",
                count: 4,
                bytes: 4096,
              },
            ],
            privacy: [
              {
                browser: "google-chrome",
                browser_name: "Google Chrome",
                profile: null,
                class: "cache",
                consequence: "regenerable",
                label: "GPU cache",
                path: "/Users/tester/Library/Application Support/Google/Chrome/Default/GPUCache",
                member_count: 1,
                is_dir: false,
                size_bytes: 1024,
                file_count: 1,
                size_is_floor: false,
                offerable: true,
                bulk_grantable: true,
                smart_scan_eligible: true,
                withheld: null,
                undisposable: null,
              },
            ],
            large_old: {
              // The module's own answer, which includes a row inside a
              // browser's data. The dispatcher refuses those outright.
              items: [
                {
                  path: "/Users/tester/Downloads/big.iso",
                  size_bytes: 2048,
                  modified_ms: null,
                },
                {
                  path: "/Users/tester/Library/Application Support/Firefox/Profiles/x/places.sqlite",
                  size_bytes: 4096,
                  modified_ms: null,
                },
              ],
              matched: 2,
              matched_bytes: 6144,
              examined: 10,
              truncated: false,
              skipped_unreadable: 0,
              skipped_hardlinked: 0,
              skipped_unrepresentable: 0,
              partial: false,
            },
            // What the gesture may be asked to act on: the browser row is not
            // in it, so the screen has no control that could name it.
            large_old_offerable: [
              {
                path: "/Users/tester/Downloads/big.iso",
                size_bytes: 2048,
                modified_ms: null,
              },
            ],
            startup: {
              starts_at_login: 0,
              can_act_on: 0,
              modern_store_present: true,
              partial: false,
            },
            permissions: {
              trash_readable: true,
              containers_readable: true,
              safari_readable: true,
              all_readable: true,
            },
          } satisfies SmartScanReport);
        }
        if (cmd === "dispatch_smart_scan") {
          ((w as Record<string, unknown>).__dispatched as unknown[]).push(args);
          return Promise.resolve({
            steps: [],
            completed: true,
            bytes_freed: 0,
            entries_freed: 0,
            actions_refused: 0,
          });
        }
        return Promise.resolve(null);
      },
      transformCallback: (cb: unknown) => cb,
    };
  });

  await page.goto("/");
  await page.getByRole("button", { name: "Scan My Mac" }).click();

  // Open the large-file chooser and tick the one row the report offers.
  await page.getByRole("button", { name: /choose files/i }).click();
  await expect(
    page.getByRole("checkbox", { name: "Choose big.iso" }),
  ).toBeVisible();
  // And the row the report did NOT offer has no control at all. This is the
  // property, not an incidental: a screen that rendered a checkbox here would
  // let a person assemble a request the backend refuses *as a whole*, for a row
  // that same report had just shown them.
  await expect(
    page.getByRole("checkbox", { name: /places\.sqlite/ }),
  ).toHaveCount(0);
  await expect(page.getByText("places.sqlite")).toHaveCount(0);
  await page.getByRole("checkbox", { name: "Choose big.iso" }).check();

  await page.getByRole("button", { name: /review & clean/i }).click();
  await page.getByRole("button", { name: "Move to Trash" }).click();
  await expect(page.getByRole("button", { name: /scan again/i })).toBeVisible();

  const calls = (await page.evaluate(
    () => (window as never as Record<string, unknown[]>).__dispatched,
  )) as { request: Record<string, unknown> }[];
  expect(calls).toHaveLength(1);
  const req = calls[0].request;

  // The Trash was on the report and is not on the request.
  expect(req.categories).toEqual(["user-logs"]);
  // Exactly the row the report offered — never the browser one beside it in
  // the walk.
  expect(req.large_old_paths).toEqual(["/Users/tester/Downloads/big.iso"]);
  expect(req.privacy_paths).toHaveLength(1);
  // Three magnitudes, never one, and none inherited from another source.
  expect(req.expected).toEqual({
    cleanup: { count: 12, bytes: 2048 },
    privacy: { count: 1, bytes: 1024 },
    large_old: { count: 1, bytes: 2048 },
  });
  // `deny_unknown_fields` on the backend would refuse an extra key; the screen
  // must not send one in the first place. `acknowledged` in particular does not
  // exist on this request, and a screen that grew one would be routing a
  // consequence through a sheet that never named it.
  expect(Object.keys(req).sort()).toEqual([
    "categories",
    "confirm_mass_delete",
    "expected",
    "filters",
    "large_old_paths",
    "privacy_paths",
    "scanned_at_ms",
  ]);
  expect(req.scanned_at_ms).toBe(1_757_000_000_000);
});
