// Render every icon this app ships, from one source: this file.
//
//   node crates/gui/src-tauri/icons/render-icons.mjs
//
// Playwright comes from crates/gui/node_modules (run `npm ci` there first).
// `iconutil` is part of macOS.
//
// Outputs (all generated — do not hand-edit any of them):
//   crates/gui/src-tauri/icons/  32x32.png 64x64.png 128x128.png
//                                128x128@2x.png icon.png icon.icns icon.ico
//                                tray-template.png
//   design/                      swept-icon.svg swept-tray-template.svg
//
// WHY A SCRIPT AND NOT A CHECKED-IN PNG. What this replaces was
// `design/icon-source.png`: a flat blue square with a white rounded square in
// it, generated for `mac-cleaner` in #18 and never touched again. It was a
// placeholder, it was not even nominally Swept's, and because it was a bitmap
// nobody could change it without redrawing it. The geometry below is computed,
// so the corner is on the macOS grid rather than near it.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "../../../.."); // repo root
const DESIGN = join(ROOT, "design");

// ---------------------------------------------------------------------------
// Palette — the same values as crates/gui/src/styles.css, so the icon and the
// app are visibly one system. `--accent-graphic` is the role used here: rings,
// bars and tints, never text.
// ---------------------------------------------------------------------------
const ACCENT = "#0A84FF"; // --accent-graphic  10 132 255
const FLANK = "#A8D5FF"; // the arc's leading edge
// One per cleaner, stable across every view — the same six the sidebar uses.
const HUES = ["#0A84FF", "#BF5AF2", "#FF9F0A", "#30D158", "#FF6482", "#64D2FF"];

// ---------------------------------------------------------------------------
// The Big Sur+ app-icon body: an 824x824 superellipse centred on 1024, leaving
// the 100px of clear margin the platform expects. n = 5 is the closest
// single-exponent fit to Apple's continuous corner — an rx-rounded rect is
// visibly rounder where the corner meets the straight edge.
// ---------------------------------------------------------------------------
function squircle(cx, cy, a, n = 5, steps = 288) {
  const p = [];
  for (let i = 0; i < steps; i++) {
    const t = (i / steps) * 2 * Math.PI;
    const c = Math.cos(t);
    const s = Math.sin(t);
    p.push(
      `${(cx + Math.sign(c) * Math.pow(Math.abs(c), 2 / n) * a).toFixed(1)} ` +
        `${(cy + Math.sign(s) * Math.pow(Math.abs(s), 2 / n) * a).toFixed(1)}`,
    );
  }
  return "M" + p.join("L") + "Z";
}
const BODY = squircle(512, 512, 412);

// The sweep is an ARC, not a chord. A straight line corner to corner reads as
// a prohibition slash before it reads as a sweep; a curve cannot be mistaken
// for one, and it carries a direction. Centre sits off the tile to the lower
// right so the arc bows toward the corner it is clearing into.
const ARC = { cx: 1500, cy: 1500, r: 1500 };

// The field is held INSIDE the body rather than laid over the whole canvas: a
// block sliced off by the squircle reads as a rendering fault, not a choice.
const FIELD_MIN = 140;
const FIELD_MAX = 884;
const CELL = (FIELD_MAX - FIELD_MIN) / 4;

/** Every block repeats the tile's own 0.225 corner ratio. */
function block({ x, y, side, fill, opacity, rot }) {
  const r = (side * 0.225).toFixed(1);
  const t = rot
    ? ` transform="rotate(${rot.toFixed(1)} ${(x + side / 2).toFixed(1)} ${(y + side / 2).toFixed(1)})"`
    : "";
  return (
    `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" ` +
    `width="${side.toFixed(1)}" height="${side.toFixed(1)}" ` +
    `rx="${r}" ry="${r}" fill="${fill}" opacity="${opacity.toFixed(3)}"${t}/>`
  );
}

/** Blocks beyond the arc, ordered biggest-first so the compact variant can
 *  simply take the front of the list. */
function field() {
  const out = [];
  for (let i = 0; i < 4; i++) {
    for (let j = 0; j < 4; j++) {
      // Jitter and a few degrees of rotation are the whole difference between
      // rubble and a Launchpad grid. Deterministic, so renders are stable.
      const jx = (((i * 7 + j * 13) % 5) - 2) * 14;
      const jy = (((i * 11 + j * 5) % 5) - 2) * 14;
      const cx = FIELD_MIN + i * CELL + CELL / 2 + jx;
      const cy = FIELD_MIN + j * CELL + CELL / 2 + jy;
      const inside = ARC.r - Math.hypot(cx - ARC.cx, cy - ARC.cy);
      if (inside <= 40) continue;
      const k = Math.min(1, inside / 497 / 0.8);
      out.push({
        k,
        cx,
        cy,
        side: 40 + 140 * k,
        fill: HUES[(i + 2 * j) % HUES.length],
        opacity: 0.5 + 0.5 * Math.min(1, k / 0.45),
        rot: (((i * 5 + j * 3) % 7) - 3) * 3,
      });
    }
  }
  return out.sort((a, b) => b.k - a.k);
}

/**
 * The app mark.
 *
 * `compact` is not a smaller copy of the same drawing — it is a different
 * drawing. Ten blocks and a 46-unit arc are correct at 128 and up; at 32 they
 * are coloured specks and a hairline. Below 128 the mark keeps its five
 * largest blocks, grows them, and thickens the arc, which is the ordinary
 * discipline for an .icns and the reason Apple ships per-size art at all.
 */
function appIcon({ compact = false } = {}) {
  const blocks = compact ? field().slice(0, 5) : field();
  const grow = compact ? 1.16 : 1;
  const arcW = compact ? 74 : 46;
  const glowW = compact ? 122 : 86;
  const flankW = compact ? 20 : 13;
  const flankR = ARC.r + (compact ? 42 : 30);

  const rects = blocks
    .map((b) => {
      const side = b.side * grow;
      return block({ ...b, side, x: b.cx - side / 2, y: b.cy - side / 2 });
    })
    .join("\n    ");

  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024" role="img" aria-label="Swept">
  <defs>
    <clipPath id="body"><path d="${BODY}"/></clipPath>
    <linearGradient id="ground" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#34343D"/>
      <stop offset="0.55" stop-color="#1F1F25"/>
      <stop offset="1" stop-color="#151519"/>
    </linearGradient>
    <radialGradient id="gloss" cx="0.26" cy="0.12" r="0.7">
      <stop offset="0" stop-color="#FFFFFF" stop-opacity="0.07"/>
      <stop offset="1" stop-color="#FFFFFF" stop-opacity="0"/>
    </radialGradient>
    <filter id="glow" x="-25%" y="-25%" width="150%" height="150%">
      <feGaussianBlur stdDeviation="17"/>
    </filter>
  </defs>
  <g clip-path="url(#body)">
    <rect width="1024" height="1024" fill="url(#ground)"/>
    <rect width="1024" height="1024" fill="url(#gloss)"/>
    ${rects}
    <circle cx="${ARC.cx}" cy="${ARC.cy}" r="${ARC.r}" fill="none" stroke="${ACCENT}" stroke-width="${glowW}" opacity="0.32" filter="url(#glow)"/>
    <circle cx="${ARC.cx}" cy="${ARC.cy}" r="${ARC.r}" fill="none" stroke="${ACCENT}" stroke-width="${arcW}"/>
    <circle cx="${ARC.cx}" cy="${ARC.cy}" r="${flankR}" fill="none" stroke="${FLANK}" stroke-width="${flankW}"/>
  </g>
  <path d="${BODY}" fill="none" stroke="#FFFFFF" stroke-opacity="0.11" stroke-width="3"/>
</svg>`;
}

/**
 * The menu-bar glyph.
 *
 * A macOS menu-bar extra takes a TEMPLATE image: a black shape plus alpha,
 * which the system recolours for the light and dark menu bar and inverts while
 * the menu is open. Any colour authored here is discarded, so the mark has to
 * survive as a silhouette at 22pt — which means heavier than the app icon and
 * reduced to two shapes. What this replaces was the sidebar's Cleanup glyph,
 * borrowed: the menu bar was wearing one module's icon as though it were the
 * whole app's.
 */
function trayIcon() {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16" role="img" aria-label="Swept">
  <circle cx="23.4" cy="23.4" r="21.83" fill="none" stroke="#000" stroke-width="2.4"/>
  <rect x="9.6" y="9.6" width="4.3" height="4.3" rx="1.1" ry="1.1" fill="#000"/>
</svg>`;
}

// ---------------------------------------------------------------------------
// A PNG-embedded .ico, written by hand: there is no ImageMagick on this
// machine and `sips` cannot write the format. Windows is not a target, but
// tauri.conf.json lists the file, so it has to exist and be valid.
// ---------------------------------------------------------------------------
function ico(pngs) {
  const head = Buffer.alloc(6);
  head.writeUInt16LE(0, 0); // reserved
  head.writeUInt16LE(1, 2); // 1 = icon
  head.writeUInt16LE(pngs.length, 4);

  let offset = 6 + pngs.length * 16;
  const dir = [];
  for (const { size, data } of pngs) {
    const e = Buffer.alloc(16);
    e.writeUInt8(size >= 256 ? 0 : size, 0); // 0 means 256
    e.writeUInt8(size >= 256 ? 0 : size, 1);
    e.writeUInt8(0, 2); // palette size
    e.writeUInt8(0, 3); // reserved
    e.writeUInt16LE(1, 4); // colour planes
    e.writeUInt16LE(32, 6); // bits per pixel
    e.writeUInt32LE(data.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += data.length;
    dir.push(e);
  }
  return Buffer.concat([head, ...dir, ...pngs.map((p) => p.data)]);
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------
const { chromium } = await import(
  resolve(ROOT, "crates/gui/node_modules/@playwright/test/index.mjs")
);

const browser = await chromium.launch();

async function png(svg, size) {
  const page = await browser.newPage({
    viewport: { width: size, height: size },
    deviceScaleFactor: 1,
  });
  // No baked shadow and no baked background: macOS draws the Dock shadow
  // itself, and the transparent margin is part of the grid.
  await page.setContent(
    `<style>html,body{margin:0;padding:0;background:transparent}` +
      `svg{width:${size}px;height:${size}px;display:block}</style>${svg}`,
  );
  const buf = await page.locator("svg").screenshot({ omitBackground: true });
  await page.close();
  return buf;
}

const FULL = appIcon();
const SMALL = appIcon({ compact: true });
const TRAY = trayIcon();
// Below 128 the compact drawing is the one that survives. 64 is the boundary
// and goes to compact: at 64 the full mark's smallest blocks are already 4px.
const art = (size) => (size >= 128 ? FULL : SMALL);

const iconset = join(HERE, "swept.iconset");
mkdirSync(iconset, { recursive: true });

const wrote = [];
const put = (path, buf) => {
  writeFileSync(path, buf);
  wrote.push(`${path.replace(ROOT + "/", "")} (${buf.length} bytes)`);
};

// The sizes tauri.conf.json names, plus the 512 the .app uses at large scale.
for (const [name, size] of [
  ["32x32.png", 32],
  ["64x64.png", 64],
  ["128x128.png", 128],
  ["128x128@2x.png", 256],
  ["icon.png", 512],
]) {
  put(join(HERE, name), await png(art(size), size));
}

// The .icns needs every slot; iconutil refuses a partial set.
for (const [name, size] of [
  ["icon_16x16.png", 16],
  ["icon_16x16@2x.png", 32],
  ["icon_32x32.png", 32],
  ["icon_32x32@2x.png", 64],
  ["icon_128x128.png", 128],
  ["icon_128x128@2x.png", 256],
  ["icon_256x256.png", 256],
  ["icon_256x256@2x.png", 512],
  ["icon_512x512.png", 512],
  ["icon_512x512@2x.png", 1024],
]) {
  writeFileSync(join(iconset, name), await png(art(size), size));
}
execFileSync("/usr/bin/iconutil", [
  "-c",
  "icns",
  iconset,
  "-o",
  join(HERE, "icon.icns"),
]);
wrote.push(
  `crates/gui/src-tauri/icons/icon.icns (${readFileSync(join(HERE, "icon.icns")).length} bytes)`,
);
// The .iconset is scaffolding for iconutil, not a product — ten more PNGs in
// the tree would just be a second, drifting copy of the same art.
rmSync(iconset, { recursive: true, force: true });

const icoSizes = [16, 32, 48, 64, 128, 256];
const icoPngs = [];
for (const size of icoSizes) {
  icoPngs.push({ size, data: await png(art(size), size) });
}
put(join(HERE, "icon.ico"), ico(icoPngs));

// 22pt is the menu-bar icon box; @2x makes it 44px.
put(join(HERE, "tray-template.png"), await png(TRAY, 44));

// The vector source, for the README, a site, or anyone who wants to edit it.
put(join(DESIGN, "swept-icon.svg"), Buffer.from(FULL + "\n", "utf8"));
put(join(DESIGN, "swept-icon-small.svg"), Buffer.from(SMALL + "\n", "utf8"));
put(join(DESIGN, "swept-tray-template.svg"), Buffer.from(TRAY + "\n", "utf8"));

await browser.close();
console.log(wrote.join("\n"));
