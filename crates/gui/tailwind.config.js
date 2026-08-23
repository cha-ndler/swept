/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      // Design tokens (see src/styles.css for the CSS variables, and
      // design/canvas/index.html for the source of truth they were ported from).
      //
      // The first block keeps the names v0.2 already used, so existing markup
      // keeps working while pointing at the measured ladder instead of the old
      // flat palette. The second adds what the ladder gained.
      colors: {
        bg: "var(--bg-window)",
        surface: "var(--surface-1)",
        border: "var(--border)",
        text: "var(--text)",
        muted: "var(--text-2)",
        accent: "var(--accent-fill)",
        danger: "var(--danger-text)",

        sidebar: "var(--sidebar)",
        surface2: "var(--surface-2)",
        surface3: "var(--surface-3)",
        separator: "var(--separator)",
        borderStrong: "var(--border-strong)",
        subtle: "var(--text-3)",
        // Never put a white label on `accentGraphic` — it is 3.65:1 on white.
        accentGraphic: "var(--accent-graphic)",
        accentText: "var(--accent-text)",
        accentTint: "var(--accent-tint)",
        success: "var(--success)",
        warning: "var(--warning)",
        dangerFill: "var(--danger-fill)",

        cat: {
          caches: "var(--cat-caches)",
          logs: "var(--cat-logs)",
          trashes: "var(--cat-trashes)",
          build: "var(--cat-build)",
          large: "var(--cat-large)",
          browser: "var(--cat-browser)",
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
