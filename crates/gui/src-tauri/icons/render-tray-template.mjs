// Render the menu-bar template icon.
//
// A macOS menu-bar extra takes a *template* image: a black shape plus alpha,
// which the system recolours for light/dark menu bars and inverts when the menu
// is open. The full-colour app icon is not one — squeezed to 22pt it reads as a
// featureless square, which is what shipped in the first build of this feature.
//
// The glyph is the same layered-stack mark the sidebar uses for Cleanup, so the
// menu bar and the app agree on what this app's symbol is.
//
//   node crates/gui/src-tauri/icons/render-tray-template.mjs
//
// Playwright comes from crates/gui/node_modules (run `npm ci` there first).

import { chromium } from "../../node_modules/@playwright/test/index.mjs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const out = resolve(here, "tray-template.png");

// 22pt is the menu-bar icon box; @2x makes it 44px.
const SIZE = 44;

const svg = `
<svg xmlns="http://www.w3.org/2000/svg" width="${SIZE}" height="${SIZE}" viewBox="0 0 16 16">
  <g fill="none" stroke="#000" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
    <path d="M8 1.9 14.4 5.5 8 9.1 1.6 5.5Z"/>
    <path d="M1.6 9 8 12.6 14.4 9"/>
  </g>
</svg>`;

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: SIZE, height: SIZE },
  deviceScaleFactor: 1,
});
await page.setContent(
  `<style>html,body{margin:0;padding:0;background:transparent}</style>${svg}`,
);
await page.locator("svg").screenshot({ path: out, omitBackground: true });
await browser.close();

console.log(`wrote ${out} (${SIZE}x${SIZE}, template)`);
