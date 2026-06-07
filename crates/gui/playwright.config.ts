import { defineConfig } from "@playwright/test";

// The UX oracle: render the built frontend in headless Chromium, capture
// screenshots (for the ux-critic / human), assert a11y (axe), and guard against
// unintended visual change (visual-regression snapshots).
export default defineConfig({
  testDir: "./ux",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: [["html", { open: "never" }], ["list"]],
  use: {
    browserName: "chromium",
    baseURL: "http://localhost:4173",
  },
  // Tolerant across machines (local mac vs CI macOS runner) — catch real changes,
  // not sub-pixel font rendering noise.
  expect: { toHaveScreenshot: { maxDiffPixelRatio: 0.1 } },
  projects: [
    { name: "desktop", use: { viewport: { width: 1200, height: 800 } } },
    { name: "narrow", use: { viewport: { width: 720, height: 800 } } },
  ],
  webServer: {
    command: "npm run preview",
    url: "http://localhost:4173",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
