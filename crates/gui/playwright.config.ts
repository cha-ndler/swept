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
    // Never reuse. `reuseExistingServer: !process.env.CI` is the Playwright
    // default and it is wrong for this repo: it only checks that *something*
    // answers on 4173, not that the something is this checkout's `dist`.
    //
    // Measured, not theorised — a `vite preview` left running in a sibling git
    // worktree served every screenshot of an entire Space Lens run. The tests
    // failed, which is the lucky outcome; had the two builds been closer, the
    // oracle would have passed while scoring a different app, and the visual
    // gate would have been protecting the wrong pixels.
    //
    // `npm run preview` uses `--strictPort`, so with reuse off an occupied
    // 4173 is a loud startup failure instead of a silent substitution. The
    // fix, when it happens, is to stop the other server.
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
