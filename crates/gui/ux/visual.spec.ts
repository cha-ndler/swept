import { test, expect } from "@playwright/test";
import type { Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { mkdirSync } from "node:fs";
import {
  SAMPLE_DISPOSE_SUMMARY,
  SAMPLE_INSTALLED_APPS,
  SAMPLE_LARGE_OLD,
  SAMPLE_LOGIN_ITEMS,
  SAMPLE_PRIVACY,
  SAMPLE_PRIVACY_COMPLETE,
  SAMPLE_PRIVACY_EMPTY,
  SAMPLE_PRIVACY_SUMMARY,
  SAMPLE_REPORT,
  SAMPLE_SMART_SCAN,
  SAMPLE_SMART_SCAN_PARTIAL,
  SAMPLE_SMART_SCAN_RUN,
  SAMPLE_SMART_SCAN_STOPPED,
  SAMPLE_SPACE_LENS,
  SAMPLE_STARTUP,
  SAMPLE_STARTUP_EMPTY,
  SAMPLE_STARTUP_SUMMARY,
  SAMPLE_SPACE_LENS_COMPLETE,
  SAMPLE_SPACE_LENS_EMPTY,
  SAMPLE_SUMMARY,
  SAMPLE_UNINSTALL,
  SAMPLE_UNINSTALL_COMPLETE,
  SAMPLE_UNINSTALL_EMPTY,
  SAMPLE_UNINSTALL_INSTALLED,
  SAMPLE_UNINSTALL_SUMMARY,
} from "./fixtures";

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
    largeOld?: unknown;
    disposeSummary?: unknown;
    spaceLens?: unknown;
    installedApps?: unknown;
    uninstall?: unknown;
    uninstallSummary?: unknown;
    /** When set, `dispose_leftovers` rejects with this message. */
    uninstallReject?: string;
    /** Hang only the leftover search, so the picker still answers. */
    hangLeftovers?: boolean;
    privacy?: unknown;
    privacySummary?: unknown;
    /** When set, `dispose_privacy` rejects with this message. */
    privacyReject?: string;
    /** Hang only the privacy scan, for the loading state. */
    hangPrivacy?: boolean;
    startup?: unknown;
    startupSummary?: unknown;
    /** When set, the startup verbs reject with this message. */
    startupReject?: string;
    hangStartup?: boolean;
    smart?: unknown;
    smartRun?: unknown;
    /** Hang only the Smart Scan, for the sweeping-ring state. */
    hangSmart?: boolean;
    /** When set, `dispatch_smart_scan` rejects with this message. */
    smartReject?: string;
  } = {},
) {
  const payload = {
    installedApps: opts.installedApps ?? SAMPLE_INSTALLED_APPS,
    uninstall: opts.uninstall ?? SAMPLE_UNINSTALL,
    uninstallSummary: opts.uninstallSummary ?? SAMPLE_UNINSTALL_SUMMARY,
    uninstallReject: opts.uninstallReject ?? null,
    hangLeftovers: opts.hangLeftovers ?? false,
    privacy: opts.privacy ?? SAMPLE_PRIVACY,
    privacySummary: opts.privacySummary ?? SAMPLE_PRIVACY_SUMMARY,
    privacyReject: opts.privacyReject ?? null,
    hangPrivacy: opts.hangPrivacy ?? false,
    startup: opts.startup ?? SAMPLE_STARTUP,
    startupSummary: opts.startupSummary ?? SAMPLE_STARTUP_SUMMARY,
    startupReject: opts.startupReject ?? null,
    hangStartup: opts.hangStartup ?? false,
    report: opts.report ?? SAMPLE_REPORT,
    items: opts.items ?? SAMPLE_LOGIN_ITEMS,
    summary: opts.summary ?? SAMPLE_SUMMARY,
    hang: opts.hang ?? false,
    // Full access unless a test says otherwise, so the notice stays out of the
    // other screenshots.
    perms: opts.perms ?? {
      trash_readable: true,
      containers_readable: true,
      safari_readable: true,
      all_readable: true,
    },
    smart: opts.smart ?? SAMPLE_SMART_SCAN,
    smartRun: opts.smartRun ?? SAMPLE_SMART_SCAN_RUN,
    hangSmart: opts.hangSmart ?? false,
    smartReject: opts.smartReject ?? null,
    largeOld: opts.largeOld ?? SAMPLE_LARGE_OLD,
    disposeSummary: opts.disposeSummary ?? SAMPLE_DISPOSE_SUMMARY,
    spaceLens: opts.spaceLens ?? SAMPLE_SPACE_LENS,
  };
  await page.addInitScript((p) => {
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = {
      invoke: (
        cmd: string,
        args: { handler?: (e: unknown) => void; paths?: string[] },
      ) => {
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
        if (cmd === "large_and_old") return Promise.resolve(p.largeOld);
        if (cmd === "dispose_paths") return Promise.resolve(p.disposeSummary);
        if (cmd === "space_lens") return Promise.resolve(p.spaceLens);
        if (cmd === "smart_scan")
          return p.hangSmart ? new Promise(() => {}) : Promise.resolve(p.smart);
        if (cmd === "dispatch_smart_scan")
          return p.smartReject
            ? Promise.reject(p.smartReject)
            : Promise.resolve(p.smartRun);
        // Best-effort in the app and swallowed there; stubbed so it does not
        // land in the unstubbed-command branch below and read as a defect.
        if (cmd === "set_tray_label") return Promise.resolve(null);
        if (cmd === "installed_apps") return Promise.resolve(p.installedApps);
        if (cmd === "uninstall_leftovers")
          return p.hangLeftovers
            ? new Promise(() => {})
            : Promise.resolve(p.uninstall);
        if (cmd === "dispose_leftovers")
          return p.uninstallReject
            ? Promise.reject(p.uninstallReject)
            : Promise.resolve(p.uninstallSummary);
        if (cmd === "privacy_report")
          return p.hangPrivacy
            ? new Promise(() => {})
            : Promise.resolve(p.privacy);
        if (cmd === "dispose_privacy") {
          if (p.privacyReject) return Promise.reject(p.privacyReject);
          // Derive the summary from the paths actually requested. A fixed
          // fixture let a baseline lock a headline and a figure from two
          // different actions — "Caches cleared / 39.0 MiB" for a selection of
          // one 14.3 MiB row — a pair the real app cannot produce.
          const chosen = (
            p.privacy as { rows: { path: string; size_bytes: number; is_dir: boolean; file_count: number }[] }
          ).rows.filter((r) =>
            ((args as { paths?: string[] }).paths ?? []).includes(r.path),
          );
          return Promise.resolve({
            dry_run: false,
            executed: chosen.length,
            refused: 0,
            bytes_freed: chosen.reduce((n, r) => n + r.size_bytes, 0),
            entries_freed: chosen
              .filter((r) => r.is_dir)
              .reduce((n, r) => n + r.file_count, 0),
          });
        }
        // The one URL the app will ever open; the screen only needs it not to
        // throw.
        if (cmd === "open_privacy_settings") return Promise.resolve(null);
        if (cmd === "open_login_items_settings") return Promise.resolve(null);
        if (cmd === "startup_report")
          return p.hangStartup
            ? new Promise(() => {})
            : Promise.resolve(p.startup);
        if (cmd === "move_aside" || cmd === "put_back")
          return p.startupReject
            ? Promise.reject(p.startupReject)
            : Promise.resolve(p.startupSummary);
        return Promise.reject(new Error(`unstubbed command: ${cmd}`));
      },
      transformCallback: (cb: unknown) => cb,
    };
  }, payload);
}

async function capture(page: Page, name: string, project: string) {
  // Park the pointer before capturing. Playwright leaves the virtual cursor
  // wherever the last click put it, so a screenshot taken after an interaction
  // records a hover state — and the reviewer then scores a frame no resting
  // user ever sees. It is the same failure as the mid-animation captures fixed
  // below, arriving through a different door: the drilled-in Space Lens shot
  // came out with two of four wedges dimmed because the cursor was still
  // sitting on the row that had just been clicked.
  await page.mouse.move(0, 0);

  // `animations: "disabled"` is load-bearing, not tidiness.
  //
  // `toHaveScreenshot` disables animations by default; a bare `page.screenshot`
  // does not. So the visual-regression gate below has always compared a settled
  // frame, while the PNG written here — the one the `ux-critic` scores and the
  // human taste gate looks at — was whatever frame of the `overlay-in`/
  // `sheet-in` fade happened to be on screen. Every confirmation-sheet
  // screenshot in this repo since the clean flow shipped has been a ghostly,
  // half-transparent capture with no backdrop dim: the review surface was an
  // artifact of the capture rather than the design.
  //
  // Playwright's own `toBeVisible()` does not help — an element at opacity 0 is
  // "visible" to it (measured: `opacity: 0` at the instant the assertion
  // passed), so waiting on the dialog never waited for the fade.
  await page.screenshot({
    path: `${SHOTS}/${name}-${project}.png`,
    fullPage: true,
    animations: "disabled",
  });

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
  await page.goto("/?tab=cleanup");
  await expect(page.getByRole("heading", { name: "Cleanup" })).toBeVisible();
  await capture(page, "scan-results", testInfo.project.name);
});

test("scan empty", async ({ page }, testInfo) => {
  await installBackend(page, {
    report: {
      ...SAMPLE_REPORT,
      total_count: 0,
      total_bytes: 0,
      by_category: [],
    },
  });
  await page.goto("/?tab=cleanup");
  await expect(page.getByText("Nothing to clean")).toBeVisible();
  await capture(page, "scan-empty", testInfo.project.name);
});

// A scan that found nothing is only good news if it could see everything. These
// two are the states that used to show a green shield and "your Mac is tidy"
// over a hole — the one claim this app must never make.
test("scan empty but blind", async ({ page }, testInfo) => {
  await installBackend(page, {
    report: {
      ...SAMPLE_REPORT,
      total_count: 0,
      total_bytes: 0,
      by_category: [],
      skipped_unreadable: 3,
      partial: true,
    },
  });
  await page.goto("/?tab=cleanup");
  await expect(
    page.getByText("Nothing found in what could be read"),
  ).toBeVisible();
  await expect(page.getByText("Your Mac is tidy")).toHaveCount(0);
  await capture(page, "scan-empty-blind", testInfo.project.name);
});

test("scan results over an incomplete walk", async ({ page }, testInfo) => {
  await installBackend(page, {
    report: { ...SAMPLE_REPORT, skipped_unreadable: 3, partial: true },
  });
  await page.goto("/?tab=cleanup");
  await expect(page.getByText("This is a floor, not a total")).toBeVisible();
  await expect(page.getByText("reclaimable, at least")).toBeVisible();
  await capture(page, "scan-results-floor", testInfo.project.name);
});

test("scan loading", async ({ page }, testInfo) => {
  await installBackend(page, { hang: true });
  await page.goto("/?tab=cleanup");
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
    perms: {
      trash_readable: false,
      containers_readable: true,
      all_readable: false,
    },
  });
  await page.goto("/?tab=cleanup");
  await expect(page.getByText(/under-reporting/i)).toBeVisible();
  // The figures shown are still real — the notice explains a gap, it does not
  // replace or qualify the numbers themselves.
  await expect(page.getByText("6.4 GiB")).toBeVisible();
  await capture(page, "scan-limited-access", testInfo.project.name);
});

// The fixture above overrides `perms` only, leaving `partial: false` — which the
// backend cannot produce, because a withheld `~/.Trash` is itself a cause of
// `partial`. It is kept as the "permissions explain everything" case; these two
// are the states a real Mac without Full Disk Access actually reaches.
test("scan results, withheld and incomplete", async ({ page }, testInfo) => {
  await installBackend(page, {
    perms: {
      trash_readable: false,
      containers_readable: true,
      all_readable: false,
    },
    report: { ...SAMPLE_REPORT, skipped_unreadable: 3, partial: true },
  });
  await page.goto("/?tab=cleanup");
  // Both notices, and the second counts the first rather than repeating it.
  await expect(page.getByText(/under-reporting/i)).toBeVisible();
  await expect(page.getByText(/Counting the above/i)).toBeVisible();
  await expect(page.getByText("reclaimable, at least")).toBeVisible();
  await capture(page, "scan-limited-access-partial", testInfo.project.name);
});

test("scan empty, withheld and incomplete", async ({ page }, testInfo) => {
  await installBackend(page, {
    perms: {
      trash_readable: false,
      containers_readable: true,
      all_readable: false,
    },
    report: {
      ...SAMPLE_REPORT,
      total_count: 0,
      total_bytes: 0,
      by_category: [],
      skipped_unreadable: 2,
      partial: true,
    },
  });
  await page.goto("/?tab=cleanup");
  // The remedy sits above the card, as it does above the results — not beneath.
  await expect(page.getByRole("button", { name: "Open Settings" })).toBeVisible();
  await expect(page.getByText("Your Mac is tidy")).toHaveCount(0);
  await capture(page, "scan-empty-limited-access", testInfo.project.name);
});

test("full access shows no notice", async ({ page }) => {
  await installBackend(page);
  await page.goto("/?tab=cleanup");
  await expect(page.getByRole("heading", { name: "Cleanup" })).toBeVisible();
  await expect(page.getByText(/under-reporting/i)).toHaveCount(0);
});

test("scan confirm", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/?tab=cleanup");
  await page.getByRole("button", { name: /review & clean/i }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await capture(page, "scan-confirm", testInfo.project.name);
});

test("scan done", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/?tab=cleanup");
  await page.getByRole("button", { name: /review & clean/i }).click();
  await page.getByRole("button", { name: /^move to/i }).click();
  await expect(page.getByText(/moved to the Trash/i)).toBeVisible();
  await expect(
    page.getByRole("button", { name: /back to cleanup/i }),
  ).toBeVisible();
  await capture(page, "scan-done", testInfo.project.name);
});

test("scan error", async ({ page }, testInfo) => {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = {
      invoke: (cmd: string) => {
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        return Promise.reject(
          "Couldn’t read ~/Library/Caches (permission denied).",
        );
      },
      transformCallback: (cb: unknown) => cb,
    };
  });
  await page.goto("/?tab=cleanup");
  await expect(page.getByText(/couldn.t finish/i)).toBeVisible();
  await capture(page, "scan-error", testInfo.project.name);
});

test("startup tab", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/?tab=startup");
  await expect(page.getByRole("heading", { name: "Startup" })).toBeVisible();
  await capture(page, "startup", testInfo.project.name);
});

// --- Large & Old -----------------------------------------------------------
//
// The one module that can act outside the cleanup allowlist, so its screenshots
// carry the load-bearing claims: nothing pre-selected, the coverage caveat
// visible, and a confirmation sheet worded harder than Cleanup's.

test("large-old results", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/?tab=large-old");
  await expect(
    page.getByRole("heading", { name: "Large & Old" }),
  ).toBeVisible();
  await expect(page.getByText("5 files")).toBeVisible();
  await capture(page, "large-old-results", testInfo.project.name);
});

test("nothing is pre-selected, and the action stays disabled until it is", async ({
  page,
}) => {
  await installBackend(page);
  await page.goto("/?tab=large-old");
  await expect(
    page.getByRole("heading", { name: "Large & Old" }),
  ).toBeVisible();

  // The safety claim, asserted rather than described: no row arrives ticked.
  const boxes = page.getByRole("checkbox");
  const n = await boxes.count();
  expect(n).toBeGreaterThan(0);
  for (let i = 0; i < n; i++) {
    await expect(boxes.nth(i)).not.toBeChecked();
  }

  const act = page.getByRole("button", { name: /to Trash…$/ });
  await expect(act).toBeDisabled();
  await expect(page.getByText("Nothing selected")).toBeVisible();

  await boxes.first().check();
  await expect(act).toBeEnabled();
});

test("large-old confirm", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/?tab=large-old");
  await page.getByRole("checkbox").first().check();
  await page.getByRole("button", { name: /to Trash…$/ }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await capture(page, "large-old-confirm", testInfo.project.name);
});

test("large-old done", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/?tab=large-old");
  await page.getByRole("checkbox").first().check();
  await page.getByRole("button", { name: /to Trash…$/ }).click();
  await page
    .getByRole("button", { name: "Move to Trash", exact: true })
    .click();
  await expect(page.getByText(/files? moved to the Trash/)).toBeVisible();
  await capture(page, "large-old-done", testInfo.project.name);
});

test("large-old empty", async ({ page }, testInfo) => {
  await installBackend(page, {
    largeOld: {
      ...SAMPLE_LARGE_OLD,
      items: [],
      matched: 0,
      matched_bytes: 0,
      skipped_unreadable: 0,
      skipped_hardlinked: 0,
      partial: false,
    },
  });
  await page.goto("/?tab=large-old");
  await expect(page.getByText("Nothing to review")).toBeVisible();
  await capture(page, "large-old-empty", testInfo.project.name);
});

test("a complete walk shows no coverage caveat", async ({ page }) => {
  await installBackend(page, {
    largeOld: {
      ...SAMPLE_LARGE_OLD,
      skipped_unreadable: 0,
      skipped_hardlinked: 0,
      skipped_unrepresentable: 0,
      truncated: false,
      partial: false,
    },
  });
  await page.goto("/?tab=large-old");
  await expect(page.getByText("5 files")).toBeVisible();
  await expect(page.getByText("This is a floor, not a total")).toHaveCount(0);
});

test("a refused disposal is surfaced, not swallowed", async ({
  page,
}, testInfo) => {
  // The backend refuses the whole request when any item no longer matches what
  // was listed. That message is the user's only signal that nothing happened,
  // so it must reach the sheet rather than being replaced by a success state.
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = {
      invoke: (cmd: string) => {
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        if (cmd === "permissions")
          return Promise.resolve({
            trash_readable: true,
            containers_readable: true,
            all_readable: true,
          });
        if (cmd === "large_and_old")
          return Promise.resolve({
            items: [
              {
                path: "/Users/tester/Downloads/big.iso",
                size_bytes: 4_294_967_296,
                modified_ms: Date.now() - 90 * 86_400_000,
              },
            ],
            matched: 1,
            matched_bytes: 4_294_967_296,
            examined: 1000,
            truncated: false,
            skipped_unreadable: 0,
            skipped_hardlinked: 0,
            skipped_unrepresentable: 0,
            partial: false,
          });
        if (cmd === "dispose_paths")
          return Promise.reject(
            "refused: 1 of 1 selected items are no longer valid, so nothing was touched.",
          );
        return Promise.reject(new Error(`unstubbed command: ${cmd}`));
      },
      transformCallback: (cb: unknown) => cb,
    };
  });
  await page.goto("/?tab=large-old");
  await page.getByRole("checkbox").first().check();
  await page.getByRole("button", { name: /to Trash…$/ }).click();
  await page
    .getByRole("button", { name: "Move to Trash", exact: true })
    .click();

  await expect(page.getByRole("alert")).toContainText("no longer valid");
  // Still on the sheet — not a success screen.
  await expect(page.getByRole("dialog")).toBeVisible();
  await capture(page, "large-old-refused", testInfo.project.name);
});

test("large-old loading", async ({ page }, testInfo) => {
  await installBackend(page, { hang: true });
  await page.goto("/?tab=large-old");
  await expect(
    page.getByRole("heading", { name: "Large & Old" }),
  ).toBeVisible();
  await capture(page, "large-old-loading", testInfo.project.name);
});

test("large-old error", async ({ page }, testInfo) => {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = {
      invoke: (cmd: string) => {
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        if (cmd === "permissions")
          return Promise.resolve({
            trash_readable: true,
            containers_readable: true,
            all_readable: true,
          });
        return Promise.reject("cannot determine home directory");
      },
      transformCallback: (cb: unknown) => cb,
    };
  });
  await page.goto("/?tab=large-old");
  await expect(page.getByRole("alert")).toContainText("look for large files");
  await capture(page, "large-old-error", testInfo.project.name);
});

// --- Space Lens ------------------------------------------------------------
//
// The module with no action at all. Its screenshots carry a different claim
// from every other view's: that there is nothing here to press. So the tests
// below assert the *absence* of a disposal affordance as hard as the other
// modules assert the presence of a confirmation — an accidental button here
// would be a bigger regression than a wrong colour, and nothing else in the
// suite would catch it.

test("space-lens results", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/?tab=space-lens");
  await expect(page.getByText("Read-only view")).toBeVisible();
  await expect(page.getByRole("button", { name: /^Library/ })).toBeVisible();
  await capture(page, "space-lens-results", testInfo.project.name);
});

test("space lens offers no way to act on anything", async ({ page }) => {
  await installBackend(page);
  await page.goto("/?tab=space-lens");
  await expect(page.getByText("Read-only view")).toBeVisible();

  // No disposal control anywhere in the module — not disabled, absent.
  const main = page.getByRole("main");
  await expect(
    main.getByRole("button", { name: /trash|discard|clean|erase/i }),
  ).toHaveCount(0);
  await expect(main.getByRole("checkbox")).toHaveCount(0);

  // And it says so in words, not only by omission.
  await expect(page.getByText(/only measures/i)).toBeVisible();
});

test("drilling in changes the breadcrumb, the hub and the list", async ({
  page,
}, testInfo) => {
  await installBackend(page);
  await page.goto("/?tab=space-lens");

  const up = page.getByRole("button", { name: "Go up one folder" });
  await expect(up).toBeDisabled();

  await page.getByRole("button", { name: /^Library/ }).click();

  // The breadcrumb names where we are, and the list is now this folder's
  // children rather than the roots.
  await expect(
    page.getByRole("navigation", { name: "Location" }),
  ).toContainText("Library");
  await expect(page.getByText("Largest inside")).toBeVisible();
  await expect(page.getByRole("button", { name: /^Steam/ })).toBeVisible();
  await expect(up).toBeEnabled();

  await capture(page, "space-lens-drilled", testInfo.project.name);

  // And back out again.
  await up.click();
  await expect(page.getByText("Largest locations")).toBeVisible();
  await expect(up).toBeDisabled();
});

test("a folder measured no deeper is not a button", async ({ page }) => {
  await installBackend(page);
  await page.goto("/?tab=space-lens");
  await page.getByRole("button", { name: /^Library/ }).click();

  // `Containers` sits at the depth cap: real bytes, no children, nowhere to
  // drill. Rendering it as a button that does nothing would be the same small
  // dishonesty as a disabled control with no disabled styling.
  await expect(page.getByRole("button", { name: /^Containers/ })).toHaveCount(
    0,
  );
  // Two of Library’\s children sit at the cap, so scope to the first.
  await expect(
    page.getByText("More inside, not measured this deep").first(),
  ).toBeVisible();
});

test("a rollup is shown as an aggregate, never as a place", async ({
  page,
}) => {
  await installBackend(page);
  await page.goto("/?tab=space-lens");
  await page.getByRole("button", { name: /^Movies/ }).click();

  await expect(page.getByText("12 more items")).toBeVisible();
  await expect(page.getByRole("button", { name: /more items/ })).toHaveCount(0);
  await expect(
    page.getByText("Smaller items, not listed separately"),
  ).toBeVisible();
});

test("a complete measurement shows no coverage caveat", async ({ page }) => {
  await installBackend(page, { spaceLens: SAMPLE_SPACE_LENS_COMPLETE });
  await page.goto("/?tab=space-lens");
  await expect(page.getByRole("button", { name: /^Library/ })).toBeVisible();
  await expect(page.getByText("This is a floor, not a total")).toHaveCount(0);
});

test("space-lens empty", async ({ page }, testInfo) => {
  await installBackend(page, { spaceLens: SAMPLE_SPACE_LENS_EMPTY });
  await page.goto("/?tab=space-lens");
  await expect(page.getByText("Nothing to measure")).toBeVisible();
  await capture(page, "space-lens-empty", testInfo.project.name);
});

test("space-lens loading", async ({ page }, testInfo) => {
  await installBackend(page, { hang: true });
  await page.goto("/?tab=space-lens");
  await expect(page.getByRole("heading", { name: "Space Lens" })).toBeVisible();
  await capture(page, "space-lens-loading", testInfo.project.name);
});

test("space-lens error", async ({ page }, testInfo) => {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = {
      invoke: (cmd: string) => {
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        if (cmd === "permissions")
          return Promise.resolve({
            trash_readable: true,
            containers_readable: true,
            all_readable: true,
          });
        return Promise.reject("cannot determine home directory");
      },
      transformCallback: (cb: unknown) => cb,
    };
  });
  await page.goto("/?tab=space-lens");
  await expect(page.getByRole("alert")).toContainText("measure your folders");
  await capture(page, "space-lens-error", testInfo.project.name);
});

// --- Applications ----------------------------------------------------------
//
// The module whose ceiling is a scan: the backend decides which rows may be
// chosen at all, and the screenshots carry that. Withheld rows are on screen
// with their reason and no checkbox; nothing is pre-selected; and the sheet
// says that a folder is a recursive removal and needs the extra confirmation.

/** Name an app that is already gone, and wait for its leftovers. */
async function openLeftovers(page: Page) {
  await page.goto("/?tab=applications");
  await page.getByLabel("Bundle identifier").fill("com.acme.notes");
  await page.getByRole("button", { name: "Look for leftovers" }).click();
  await expect(page.getByText(/items to review/)).toBeVisible();
}

test("applications picker", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/?tab=applications");
  await expect(
    page.getByRole("heading", { name: "Applications" }),
  ).toBeVisible();
  await expect(page.getByText("Which application?")).toBeVisible();
  await expect(
    page.getByRole("button", { name: /Example Reader/ }),
  ).toBeVisible();
  await capture(page, "applications-pick", testInfo.project.name);
});

test("applications results", async ({ page }, testInfo) => {
  await installBackend(page);
  await openLeftovers(page);
  await expect(page.getByText("7 items to review")).toBeVisible();
  await capture(page, "applications-results", testInfo.project.name);
});

test("withheld rows carry no checkbox, and nothing is pre-selected", async ({
  page,
}) => {
  await installBackend(page);
  await openLeftovers(page);

  // Exactly the offerable rows are controls — no more, no fewer — and none
  // arrives ticked.
  const boxes = page.getByRole("checkbox");
  await expect(boxes).toHaveCount(7);
  for (let i = 0; i < 7; i++) {
    await expect(boxes.nth(i)).not.toBeChecked();
  }

  // The withheld rows are on screen, each with its reason: the user's own
  // documents, a still-installed sibling, a shared group container, and a
  // tree disposal is certain to refuse.
  await expect(
    page.getByText(/only copy — so it is shown, not offered/),
  ).toBeVisible();
  await expect(
    page.getByText(/still installed and this is its data/),
  ).toBeVisible();
  await expect(page.getByText(/shared between apps/)).toBeVisible();
  await expect(page.getByText(/this tool cannot remove it/)).toBeVisible();

  const act = page.getByRole("button", { name: /to Trash…$/ });
  await expect(act).toBeDisabled();
  await expect(page.getByText("Nothing selected")).toBeVisible();

  await boxes.first().check();
  await expect(act).toBeEnabled();
});

test("applications confirm", async ({ page }, testInfo) => {
  await installBackend(page);
  await openLeftovers(page);
  // The first offerable row is a folder, so the sheet must say what that
  // means before the button is pressed.
  await page.getByRole("checkbox").first().check();
  await page.getByRole("button", { name: /to Trash…$/ }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByText(/recursive removal/)).toBeVisible();
  await capture(page, "applications-confirm", testInfo.project.name);
});

test("applications done", async ({ page }, testInfo) => {
  await installBackend(page);
  await openLeftovers(page);
  await page.getByRole("checkbox").first().check();
  await page.getByRole("button", { name: /to Trash…$/ }).click();
  await page
    .getByRole("button", { name: "Move to Trash", exact: true })
    .click();
  await expect(page.getByText(/moved to the Trash/)).toBeVisible();
  await capture(page, "applications-done", testInfo.project.name);
});

test("applications still installed", async ({ page }, testInfo) => {
  // Picking an installed app records its identity and says what to do next;
  // it offers nothing, because an installed app has no leftovers.
  await installBackend(page, { uninstall: SAMPLE_UNINSTALL_INSTALLED });
  await page.goto("/?tab=applications");
  await page.getByRole("button", { name: /Example Reader/ }).click();
  await expect(page.getByText("Still installed")).toBeVisible();
  await expect(page.getByRole("checkbox")).toHaveCount(0);
  await capture(page, "applications-installed", testInfo.project.name);
});

test("applications empty", async ({ page }, testInfo) => {
  await installBackend(page, { uninstall: SAMPLE_UNINSTALL_EMPTY });
  await page.goto("/?tab=applications");
  await page.getByLabel("Bundle identifier").fill("com.contoso.sync");
  await page.getByRole("button", { name: "Look for leftovers" }).click();
  await expect(page.getByText("Nothing left behind")).toBeVisible();
  await capture(page, "applications-empty", testInfo.project.name);
});

test("a complete leftover search shows no coverage caveat", async ({
  page,
}) => {
  await installBackend(page, { uninstall: SAMPLE_UNINSTALL_COMPLETE });
  await openLeftovers(page);
  await expect(page.getByText("This is a floor, not a total")).toHaveCount(0);
});

test("a refused leftover disposal is surfaced, not swallowed", async ({
  page,
}, testInfo) => {
  await installBackend(page, {
    uninstallReject:
      "refused: 1 of 1 selected items are not something this scan offers, so nothing was touched.",
  });
  await openLeftovers(page);
  await page.getByRole("checkbox").first().check();
  await page.getByRole("button", { name: /to Trash…$/ }).click();
  await page
    .getByRole("button", { name: "Move to Trash", exact: true })
    .click();

  await expect(page.getByRole("alert")).toContainText("nothing was touched");
  await expect(page.getByRole("dialog")).toBeVisible();
  await capture(page, "applications-refused", testInfo.project.name);
});

test("applications loading", async ({ page }, testInfo) => {
  await installBackend(page, { hang: true });
  await page.goto("/?tab=applications");
  await expect(
    page.getByRole("heading", { name: "Applications" }),
  ).toBeVisible();
  await capture(page, "applications-loading", testInfo.project.name);
});

test("applications scanning", async ({ page }, testInfo) => {
  // The state a user watches while a real disk walk runs — distinct from the
  // picker's own loading state, and announced rather than silent.
  await installBackend(page, { hangLeftovers: true });
  await page.goto("/?tab=applications");
  await page.getByRole("button", { name: /Example Reader/ }).click();
  await expect(
    page.getByRole("status", { name: "Looking for leftovers" }),
  ).toBeVisible();
  await capture(page, "applications-scanning", testInfo.project.name);
});

test("applications error", async ({ page }, testInfo) => {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = {
      invoke: (cmd: string) => {
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        if (cmd === "permissions")
          return Promise.resolve({
            trash_readable: true,
            containers_readable: true,
            all_readable: true,
          });
        return Promise.reject("cannot determine home directory");
      },
      transformCallback: (cb: unknown) => cb,
    };
  });
  await page.goto("/?tab=applications");
  await expect(page.getByRole("alert")).toContainText(
    "list your applications",
  );
  // Naming an app by hand still works when the list could not be read — and
  // its own failure is reported the same way.
  await page.getByLabel("Bundle identifier").fill("com.acme.notes");
  await page.getByRole("button", { name: "Look for leftovers" }).click();
  await expect(page.getByRole("alert")).toContainText("look for leftovers");
  await capture(page, "applications-error", testInfo.project.name);
});

// --- Privacy ---------------------------------------------------------------

/** Open Privacy and wait for the list rather than a spinner. */
async function openPrivacy(page: Page) {
  await page.goto("/?tab=privacy");
  await expect(page.getByText(/items to review/)).toBeVisible();
}

test("privacy results", async ({ page }, testInfo) => {
  await installBackend(page);
  await openPrivacy(page);
  await capture(page, "privacy-results", testInfo.project.name);
});

/**
 * The rule this screen shares with every other: nothing is pre-chosen, and only
 * the rows the backend offers are controls at all. A withheld row — website
 * storage, or anything a running browser is holding open — is information.
 */
test("only the offerable rows are controls, and none is ticked", async ({
  page,
}) => {
  await installBackend(page);
  await openPrivacy(page);
  const boxes = page.getByRole("checkbox");
  await expect(boxes).toHaveCount(5);
  for (const b of await boxes.all()) await expect(b).not.toBeChecked();
});

/**
 * The distinctive part, and the reason this screen exists in this shape: the
 * primary action stays disabled until every consequence in the selection has
 * been acknowledged separately. `dispose_privacy` refuses an unacknowledged
 * one outright, so a sheet that did not ask would produce a refusal the user
 * could not act on.
 */
test("the sheet will not act until each consequence is acknowledged", async ({
  page,
}, testInfo) => {
  await installBackend(page);
  await openPrivacy(page);
  await page.getByRole("checkbox", { name: /Select Cookies/ }).first().check();
  await page
    .getByRole("checkbox", { name: /Select Session backups/ })
    .first()
    .check();
  await page.getByRole("button", { name: /Move .* to Trash…/ }).click();

  const act = page.getByRole("button", { name: "Move to Trash" });
  await expect(act).toBeDisabled();
  await capture(page, "privacy-confirm", testInfo.project.name);

  // One of the two is not enough: each axis is its own promise.
  await page.getByRole("checkbox", { name: "Signs you out" }).check();
  await expect(act).toBeDisabled();
  await page.getByRole("checkbox", { name: "Loses open tabs" }).check();
  await expect(act).toBeEnabled();
});

test("privacy done", async ({ page }, testInfo) => {
  await installBackend(page);
  await openPrivacy(page);
  await page.getByRole("checkbox", { name: /Select GPU cache/ }).first().check();
  await page.getByRole("button", { name: /Move .* to Trash…/ }).click();
  await page.getByRole("button", { name: "Move to Trash" }).click();
  // The done state headlines what changed for the user, not a byte figure —
  // which on the screen that argues size is not the point is the whole thesis
  // surviving to the last step.
  await expect(
    page.getByRole("heading", { name: "Caches cleared" }),
  ).toBeVisible();
  await capture(page, "privacy-done", testInfo.project.name);
});

/** A refusal is surfaced with the backend's own sentence, never swallowed. */
test("a refused privacy disposal says so and offers to look again", async ({
  page,
}, testInfo) => {
  await installBackend(page, {
    privacyReject:
      "refused: 1 of 1 selected items could not be acted on, so nothing was touched. Scan again and review.",
  });
  await openPrivacy(page);
  await page.getByRole("checkbox", { name: /Select GPU cache/ }).first().check();
  await page.getByRole("button", { name: /Move .* to Trash…/ }).click();
  await page.getByRole("button", { name: "Move to Trash" }).click();
  await expect(page.getByText("Nothing was removed")).toBeVisible();
  // Scoped to the sheet: the toolbar carries a "Look again" of its own, and
  // the one that matters here is the sheet's, because after a refusal the list
  // is stale against the backend's own re-scan.
  await expect(
    page.getByRole("dialog").getByRole("button", { name: "Look again" }),
  ).toBeVisible();
  await capture(page, "privacy-refused", testInfo.project.name);
});

/** Denied is not absent, and the screen says which one it is. */
test("a browser behind Full Disk Access is named, with a way to fix it", async ({
  page,
}) => {
  await installBackend(page);
  await openPrivacy(page);
  // Said once, in the section a reader looks in for Safari, with the button
  // that fixes it — and the reason the headline figure is a floor stated beside
  // the figure rather than in a second banner saying the same thing.
  await expect(
    page.getByText(/will not let this app read Safari's data/),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Open Settings" })).toBeVisible();
  await expect(
    page.getByText("Safari not searched — needs Full Disk Access"),
  ).toBeVisible();
});

test("privacy with nothing denied shows no caveat", async ({ page }) => {
  await installBackend(page, { privacy: SAMPLE_PRIVACY_COMPLETE });
  await openPrivacy(page);
  await expect(page.getByText(/Full Disk Access/)).toHaveCount(0);
  await expect(page.getByText(/floor rather than a total/)).toHaveCount(0);
});

test("privacy empty", async ({ page }, testInfo) => {
  await installBackend(page, { privacy: SAMPLE_PRIVACY_EMPTY });
  await page.goto("/?tab=privacy");
  await expect(page.getByText("Nothing to clear")).toBeVisible();
  await capture(page, "privacy-empty", testInfo.project.name);
});

test("privacy loading", async ({ page }, testInfo) => {
  await installBackend(page, { hangPrivacy: true });
  await page.goto("/?tab=privacy");
  await expect(page.getByText(/Looking through your browsers/)).toBeVisible();
  await capture(page, "privacy-loading", testInfo.project.name);
});

/**
 * The filter's other two states, which nothing captured before — and they hold
 * the entire surface of the withheld treatment: the hoisted reasons, the rows
 * that recede rather than being raised, and on a real machine the majority of
 * what the module found.
 */
test("privacy withheld", async ({ page }, testInfo) => {
  await installBackend(page);
  await openPrivacy(page);
  await page.getByRole("radio", { name: /Not offered/ }).click();
  await capture(page, "privacy-withheld", testInfo.project.name);
});

test("privacy all", async ({ page }, testInfo) => {
  await installBackend(page);
  await openPrivacy(page);
  await page.getByRole("radio", { name: "All" }).click();
  await capture(page, "privacy-all", testInfo.project.name);
});

/** Hidden rows are named per group, not left to be inferred from a chip. */
test("the filter says what it is holding back, per browser", async ({
  page,
}) => {
  await installBackend(page);
  await openPrivacy(page);
  // Firefox is not running, so it carries no "looks like it is running" chip;
  // before this, its 66 MiB of website storage vanished with no trace at all.
  const say = page.getByRole("button", {
    name: /1 not offered — website storage/,
  });
  await expect(say).toBeVisible();
  await say.click();
  await expect(page.getByRole("radio", { name: "All" })).toBeChecked();
});

/**
 * The multi-consequence done headline — the case the past-tense summary exists
 * for, and the one that has to survive 720px.
 */
test("privacy done with several consequences", async ({ page }, testInfo) => {
  await installBackend(page);
  await openPrivacy(page);
  await page.getByRole("checkbox", { name: /Select Cookies/ }).first().check();
  await page
    .getByRole("checkbox", { name: /Select Session backups/ })
    .first()
    .check();
  await page.getByRole("checkbox", { name: /Select GPU cache/ }).first().check();
  await page.getByRole("button", { name: /Move .* to Trash…/ }).click();
  await page.getByRole("checkbox", { name: "Signs you out" }).check();
  await page.getByRole("checkbox", { name: "Loses open tabs" }).check();
  await page.getByRole("button", { name: "Move to Trash" }).click();
  await expect(
    page.getByRole("heading", {
      name: "Signed out · Sessions cleared · Caches cleared",
    }),
  ).toBeVisible();
  await capture(page, "privacy-done-multi", testInfo.project.name);
});

// --- Startup ---------------------------------------------------------------

async function openStartup(page: Page) {
  await page.goto("/?tab=startup");
  await expect(
    page.getByRole("heading", { name: /when you log in/ }),
  ).toBeVisible();
}

test("startup results", async ({ page }, testInfo) => {
  await installBackend(page);
  await openStartup(page);
  await capture(page, "startup-results", testInfo.project.name);
});

/**
 * The ratio this screen is built around: on a real machine the jobs it cannot
 * touch outnumber the ones it can. They are a collapsed table with no controls,
 * because a row with a dead control reads as a refusal.
 */
test("what macOS manages is a table with no controls", async ({ page }) => {
  await installBackend(page);
  await openStartup(page);

  const boxes = page.getByRole("checkbox");
  await expect(boxes).toHaveCount(5); // 4 offerable + 1 set aside; never the 3 system jobs
  for (const b of await boxes.all()) await expect(b).not.toBeChecked();

  await expect(page.getByText("com.vendor1.driver")).toHaveCount(0);
  await page.getByRole("button", { name: /more macOS manages/ }).click();
  await expect(page.getByText("com.vendor1.driver")).toBeVisible();
});

/** The disclosure that reframes the count sits above it, with its route. */
test("the login items macOS keeps to itself are named first", async ({
  page,
}) => {
  await installBackend(page);
  await openStartup(page);
  await expect(
    page.getByText(/register their login items with macOS directly/),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Open Login Items & Extensions" }),
  ).toBeVisible();
});

test("startup confirm", async ({ page }, testInfo) => {
  await installBackend(page);
  await openStartup(page);
  await page
    .getByRole("checkbox", { name: /Set aside com.acme.notes.helper/ })
    .check();
  await page.getByRole("button", { name: /Set 1 aside…/ }).click();
  // The sentence people would otherwise report as a bug.
  await expect(page.getByText(/takes effect at your/)).toBeVisible();
  await capture(page, "startup-confirm", testInfo.project.name);
});

test("startup done", async ({ page }, testInfo) => {
  await installBackend(page);
  await openStartup(page);
  await page
    .getByRole("checkbox", { name: /Set aside com.acme.notes.helper/ })
    .check();
  await page.getByRole("button", { name: /Set 1 aside…/ }).click();
  await page.getByRole("button", { name: "Set aside", exact: true }).click();
  // The figure is its own mono line above the heading, as the house does it.
  await expect(page.getByRole("heading", { name: "set aside" })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Put it back" }),
  ).toBeVisible();
  await capture(page, "startup-done", testInfo.project.name);
});

/** A selection belongs to one verb; picking the other starts a new one. */
test("choosing a set-aside item switches the verb", async ({ page }) => {
  await installBackend(page);
  await openStartup(page);
  await page
    .getByRole("checkbox", { name: /Set aside com.acme.notes.helper/ })
    .check();
  await expect(page.getByRole("button", { name: /Set 1 aside…/ })).toBeVisible();

  await page
    .getByRole("checkbox", { name: /Put back com.example.reader.autostart/ })
    .check();
  await expect(page.getByRole("button", { name: /Put 1 back…/ })).toBeVisible();
});

test("a refused startup change says so and offers to look again", async ({
  page,
}, testInfo) => {
  await installBackend(page, {
    startupReject:
      "refused: 1 of 1 selected items could not be acted on, so nothing was changed.",
  });
  await openStartup(page);
  await page
    .getByRole("checkbox", { name: /Set aside com.acme.notes.helper/ })
    .check();
  await page.getByRole("button", { name: /Set 1 aside…/ }).click();
  await page.getByRole("button", { name: "Set aside", exact: true }).click();
  // The sheet's own title, not the refusal sentence that also contains the
  // phrase — the point is that the dialog stops being a pending question.
  await expect(
    page.getByRole("heading", { name: "Nothing was changed" }),
  ).toBeVisible();
  await expect(
    page.getByRole("dialog").getByRole("button", { name: "Look again" }),
  ).toBeVisible();
  await capture(page, "startup-refused", testInfo.project.name);
});

test("startup empty", async ({ page }, testInfo) => {
  await installBackend(page, { startup: SAMPLE_STARTUP_EMPTY });
  await page.goto("/?tab=startup");
  await expect(page.getByText(/Nothing is kept as a file/)).toBeVisible();
  await capture(page, "startup-empty", testInfo.project.name);
});

test("startup loading", async ({ page }, testInfo) => {
  await installBackend(page, { hangStartup: true });
  await page.goto("/?tab=startup");
  await expect(page.getByText(/Looking at what starts/)).toBeVisible();
  await capture(page, "startup-loading", testInfo.project.name);
});

// ---------------------------------------------------------------------------
// Smart Scan
// ---------------------------------------------------------------------------

// Also the gate on which module the app opens. `goto("/")` with no `?tab`
// is the real first-run path, and this is the only test that takes it.
test("smart scan idle", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Smart Scan" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Ready to scan" })).toBeVisible();
  // Nothing was scanned, so nothing claims a figure.
  await expect(page.getByText("6.4 GiB")).toHaveCount(0);
  await capture(page, "smart-scan-idle", testInfo.project.name);
});

test("smart scan scanning", async ({ page }, testInfo) => {
  await installBackend(page, { hangSmart: true });
  await page.goto("/");
  await page.getByRole("button", { name: "Scan My Mac" }).click();
  await expect(page.getByText("Scanning…")).toBeVisible();
  await capture(page, "smart-scan-scanning", testInfo.project.name);
});

test("smart scan results", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Scan My Mac" }).click();
  await expect(
    page.getByRole("button", { name: /review & clean/i }),
  ).toBeVisible();
  // The Trash is on the report and is never tickable — it is the way back from
  // everything else in the list.
  await expect(
    page.getByRole("checkbox", { name: "Include Trash" }),
  ).toHaveCount(0);
  await capture(page, "smart-scan-results", testInfo.project.name);
});

// The headline is a floor, attributed per source, with the permission notice
// above it saying which of them macOS is responsible for.
test("smart scan floor", async ({ page }, testInfo) => {
  await installBackend(page, { smart: SAMPLE_SMART_SCAN_PARTIAL });
  await page.goto("/");
  await page.getByRole("button", { name: "Scan My Mac" }).click();
  await expect(page.getByText("This is a floor, not a total")).toBeVisible();
  await expect(page.getByText("reclaimable, at least")).toBeVisible();
  await capture(page, "smart-scan-floor", testInfo.project.name);
});

test("smart scan confirm", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Scan My Mac" }).click();
  await page.getByRole("button", { name: /review & clean/i }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await capture(page, "smart-scan-confirm", testInfo.project.name);
});

test("smart scan done", async ({ page }, testInfo) => {
  await installBackend(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Scan My Mac" }).click();
  await page.getByRole("button", { name: /review & clean/i }).click();
  await page.getByRole("button", { name: "Move to Trash" }).click();
  await expect(page.getByRole("button", { name: /scan again/i })).toBeVisible();
  await capture(page, "smart-scan-done", testInfo.project.name);
});

// The state the ledger exists for: one step ran, one refused, and the third was
// never attempted — three facts a "partially succeeded" boolean would flatten.
test("smart scan stopped", async ({ page }, testInfo) => {
  await installBackend(page, { smartRun: SAMPLE_SMART_SCAN_STOPPED });
  await page.goto("/");
  await page.getByRole("button", { name: "Scan My Mac" }).click();
  await page.getByRole("button", { name: /review & clean/i }).click();
  await page.getByRole("button", { name: "Move to Trash" }).click();
  await expect(page.getByText(/Not attempted:/)).toBeVisible();
  await capture(page, "smart-scan-stopped", testInfo.project.name);
});

// A refusal before any step ran comes back to the sheet with the reason on it,
// rather than to a ledger that would have nothing to show.
test("smart scan refused", async ({ page }, testInfo) => {
  await installBackend(page, {
    smartReject:
      "refused: this report is 14 minutes old. Scan again and review.",
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Scan My Mac" }).click();
  await page.getByRole("button", { name: /review & clean/i }).click();
  await page.getByRole("button", { name: "Move to Trash" }).click();
  await expect(page.getByText(/14 minutes old/)).toBeVisible();
  await capture(page, "smart-scan-refused", testInfo.project.name);
});
