import type { ReactNode } from "react";

/**
 * Shell chrome shared by every module view.
 *
 * The sidebar lives in App; this file holds the pieces a view needs so that
 * adding a module does not mean re-deriving the toolbar or the iconography.
 */

/** SF-Symbols-flavoured line icons: 16px box, 1.5 stroke, round caps. */
function Icon({ children, size = 16 }: { children: ReactNode; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export function StackIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M8 1.8 14.5 5.4 8 9 1.5 5.4Z" />
      <path d="M1.5 8.9 8 12.5l6.5-3.6" />
      <path d="M1.5 12.2 8 15.8l6.5-3.6" />
    </Icon>
  );
}

/** Stacked documents — the Large & Old module. Files, not junk. */
export function FilesIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M4.2 1.9h4.4l3.2 3.2v7.1a.9.9 0 0 1-.9.9H4.2a.9.9 0 0 1-.9-.9V2.8a.9.9 0 0 1 .9-.9Z" />
      <path d="M8.6 1.9v3.2h3.2" />
      <path d="M13.6 4.6v8.5a1.9 1.9 0 0 1-1.9 1.9H5.4" />
    </Icon>
  );
}

export function WrenchIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M10.3 1.9a3.9 3.9 0 0 0-3.6 5.4L2 12l2 2 4.7-4.7a3.9 3.9 0 0 0 5.4-3.6c0-.6-.1-1.1-.3-1.6l-2.3 2.3-2-2 2.3-2.3c-.5-.2-1-.2-1.5-.2Z" />
    </Icon>
  );
}

export function ShieldIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M8 1.6 13.4 3.7v4.6c0 3.2-2.2 5.6-5.4 6.8-3.2-1.2-5.4-3.6-5.4-6.8V3.7Z" />
      <path d="M5.7 8.2 7.3 9.8l3.2-3.4" />
    </Icon>
  );
}

export function LockIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <rect x="2.9" y="6.9" width="10.2" height="7.4" rx="1.6" />
      <path d="M5.4 6.9V5.1a2.6 2.6 0 0 1 5.2 0v1.8" />
    </Icon>
  );
}

export function InfoIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <circle cx="8" cy="8" r="6.4" />
      <path d="M8 7.3v4.1M8 4.7v.1" />
    </Icon>
  );
}

/**
 * The top strip of a module view.
 *
 * `data-tauri-drag-region` on the flexible spacer is what makes the window
 * movable: `titleBarStyle: "Overlay"` removes the OS title bar, and Tauri only
 * treats an element as a drag handle if the attribute is on the event target
 * itself — so it goes on the empty stretch, never on a parent of the controls.
 */
export function Toolbar({
  title,
  children,
}: {
  title: string;
  children?: ReactNode;
}) {
  return (
    <header className="flex h-[52px] flex-none items-center gap-2.5 border-b border-separator px-5">
      <h1 className="text-title font-semibold">{title}</h1>
      <div className="h-full flex-1" data-tauri-drag-region />
      {children}
    </header>
  );
}

/** An inset grouped list: one card, hairline-separated rows. */
export function Group({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={`overflow-hidden rounded-card border border-separator bg-surface ${className}`}
    >
      {children}
    </div>
  );
}

/** A quiet contextual note — used for the safety statements, never for errors. */
export function Banner({
  icon,
  tone = "neutral",
  children,
}: {
  icon: ReactNode;
  tone?: "neutral" | "safe" | "consent";
  children: ReactNode;
}) {
  const tones = {
    neutral: "border-separator bg-surface text-muted",
    safe: "border-separator bg-surface text-muted",
    consent: "border-cat-trashes/30 bg-cat-trashes/[.07] text-muted",
  } as const;
  const iconTones = {
    neutral: "text-subtle",
    safe: "text-success",
    consent: "text-cat-trashes",
  } as const;
  return (
    <div
      className={`flex gap-2.5 rounded-card border px-3.5 py-2.5 text-caption leading-relaxed ${tones[tone]}`}
    >
      <span className={`mt-px flex-none ${iconTones[tone]}`}>{icon}</span>
      <div>{children}</div>
    </div>
  );
}
