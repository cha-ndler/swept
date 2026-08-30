/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      // Design tokens (see src/styles.css for the CSS variables, and
      // design/canvas/index.html for the source of truth they were ported from).
      //
      // Solid colours MUST be written `rgb(var(--x) / <alpha-value>)`, never a
      // bare `var(--x)`. Tailwind cannot split a `var()` holding a hex string
      // into channels, so `bg-cat-large/40` emitted **no CSS at all** and the
      // element rendered transparent — with no build error and no test failure.
      // 13 of the 16 opacity-modified colour classes in the app were dead this
      // way before it was measured. `src/styles.css` therefore stores channels;
      // the `<alpha-value>` placeholder is what makes the `/40` suffix work.
      //
      // The first block keeps the names v0.2 already used, so existing markup
      // keeps working while pointing at the measured ladder instead of the old
      // flat palette. The second adds what the ladder gained.
      colors: {
        bg: "rgb(var(--bg-window) / <alpha-value>)",
        surface: "rgb(var(--surface-1) / <alpha-value>)",
        border: "var(--border)",
        text: "rgb(var(--text) / <alpha-value>)",
        muted: "rgb(var(--text-2) / <alpha-value>)",
        accent: "rgb(var(--accent-fill) / <alpha-value>)",
        danger: "rgb(var(--danger-text) / <alpha-value>)",

        sidebar: "rgb(var(--sidebar) / <alpha-value>)",
        surface2: "rgb(var(--surface-2) / <alpha-value>)",
        surface3: "rgb(var(--surface-3) / <alpha-value>)",
        separator: "var(--separator)",
        borderStrong: "var(--border-strong)",
        subtle: "rgb(var(--text-3) / <alpha-value>)",
        // Never put a white label on `accentGraphic` — it is 3.65:1 on white.
        accentGraphic: "rgb(var(--accent-graphic) / <alpha-value>)",
        accentText: "rgb(var(--accent-text) / <alpha-value>)",
        accentTint: "var(--accent-tint)",
        success: "rgb(var(--success) / <alpha-value>)",
        warning: "rgb(var(--warning) / <alpha-value>)",
        dangerFill: "rgb(var(--danger-fill) / <alpha-value>)",

        cat: {
          caches: "rgb(var(--cat-caches) / <alpha-value>)",
          logs: "rgb(var(--cat-logs) / <alpha-value>)",
          trashes: "rgb(var(--cat-trashes) / <alpha-value>)",
          build: "rgb(var(--cat-build) / <alpha-value>)",
          large: "rgb(var(--cat-large) / <alpha-value>)",
          browser: "rgb(var(--cat-browser) / <alpha-value>)",
        },
      },
      // The macOS system scale. Added rather than replacing Tailwind's defaults
      // so the view internals can migrate view-by-view.
      fontSize: {
        micro: ["11px", { lineHeight: "14px", letterSpacing: ".07em" }],
        caption: ["12px", { lineHeight: "16px" }],
        body: ["13px", { lineHeight: "18px" }],
        emph: ["15px", { lineHeight: "20px" }],
        title: ["17px", { lineHeight: "22px", letterSpacing: "-.01em" }],
        display: ["28px", { lineHeight: "32px", letterSpacing: "-.02em" }],
        hero: ["52px", { lineHeight: "52px", letterSpacing: "-.025em" }],
        heroUnit: ["24px", { lineHeight: "28px" }],
      },
      borderRadius: {
        control: "6px",
        card: "10px",
        panel: "14px",
        xl: "12px",
      },
      boxShadow: {
        e2: "var(--e2)",
        e3: "var(--e3)",
      },
      transitionTimingFunction: {
        mac: "var(--ease)",
      },
      transitionDuration: {
        fast: "150ms",
        sheet: "220ms",
      },
    },
  },
  plugins: [],
};
