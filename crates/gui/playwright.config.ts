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
  // Tolerant enough for sub-pixel font rendering differences between a local
  // mac and the CI runner, but no more. The previous 0.1 (10% of a 1200x800
  // page = ~96k pixels) was loose enough that replacing an entire component
  // passed unnoticed, which defeats the point of the gate. Antialiasing noise
  // is a few hundred pixels; 0.01 leaves ~9.6k of headroom.
  expect: { toHaveScreenshot: { maxDiffPixelRatio: 0.01, threshold: 0.05 } },
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
