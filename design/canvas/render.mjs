// Export each artboard of the design canvas to design/references/ as a PNG.
//
// These PNGs are the FIRST-PARTY exemplars the `ux-critic` subagent scores
// against. The folder used to be intentionally empty because the only obvious
// references were competitors' copyrighted screenshots; generating our own from
// a canvas we control removes that problem and gives the critic something
// concrete to compare a real screenshot to.
//
//   node design/canvas/render.mjs
//
// Playwright comes from crates/gui/node_modules (run `npm ci` there first).

import { chromium } from "../../crates/gui/node_modules/@playwright/test/index.mjs";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const canvas = resolve(here, "index.html");
const outDir = resolve(here, "..", "references");

// [board id, selector to capture within that board]
const BOARDS = [
  ["foundations", null],
  ["shell", ".window"],
  ["smart-scan-idle", ".window"],
  ["smart-scan-scanning", ".window"],
  ["smart-scan-results", ".window"],
  ["confirm-sheet", ":scope > .two"],
  ["module-large-old", ".window"],
  ["space-lens", ".window"],
  ["onboarding-fda", ".window"],
  ["states", ":scope > .three"],
];

mkdirSync(outDir, { recursive: true });

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1240, height: 900 },
  deviceScaleFactor: 2,
  colorScheme: "dark",
  reducedMotion: "reduce", // freeze the sweep so renders are deterministic
});

await page.goto(pathToFileURL(canvas).href, { waitUntil: "load" });
await page.waitForTimeout(250); // let webfont fallback + SVG <use> settle

let n = 0;
for (const [id, sel] of BOARDS) {
  const board = page.locator(`#${id}`);
  const target = sel === null ? board : sel === ".window" ? board.locator(".window") : board.locator(sel).first();
  const path = resolve(outDir, `artboard-${String(++n).padStart(2, "0")}-${id}.png`);
  await target.screenshot({ path });
  const box = await target.boundingBox();
  console.log(`  ${String(n).padStart(2, "0")}  ${id.padEnd(22)} ${Math.round(box.width)}x${Math.round(box.height)}`);
}

await browser.close();
console.log(`\n${n} artboards exported to design/references/`);
