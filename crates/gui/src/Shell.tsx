import { useState } from "react";
import type { ReactNode } from "react";
import { call } from "./backend";
import type { Permissions } from "./types";

/**
 * Shell chrome shared by every module view.
 *
 * The sidebar lives in App; this file holds the pieces a view needs so that
 * adding a module does not mean re-deriving the toolbar or the iconography.
 */

/** SF-Symbols-flavoured line icons: 16px box, 1.5 stroke, round caps. */
export function Icon({
  children,
  size = 16,
}: {
  children: ReactNode;
  size?: number;
}) {
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

/** A grid of app tiles — Applications. What is installed, and what one left. */
export function AppsIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <rect x="2" y="2" width="4.8" height="4.8" rx="1.1" />
      <rect x="9.2" y="2" width="4.8" height="4.8" rx="1.1" />
      <rect x="2" y="9.2" width="4.8" height="4.8" rx="1.1" />
      <rect x="9.2" y="9.2" width="4.8" height="4.8" rx="1.1" />
    </Icon>
  );
}

/** The aperture — Space Lens. Something you look through, not something you use. */
export function LensIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <circle cx="8" cy="8" r="6.3" />
      <circle cx="8" cy="8" r="2.2" />
      <path d="M8 1.7v3.9M8 10.4v3.9M1.7 8h3.9M10.4 8h3.9" />
    </Icon>
  );
}

/** Breadcrumb separator and the "go up" affordance. */
export function ChevronIcon({
  size,
  dir = "right",
}: {
  size?: number;
  dir?: "left" | "right";
}) {
  return (
    <Icon size={size}>
      <path d={dir === "right" ? "M6 3.5 10.5 8 6 12.5" : "M10 3.5 5.5 8 10 12.5"} />
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

/**
 * Privacy. A domino mask rather than an eye: an eye reads as "watch" or
 * "preview", and this module is about what is *not* seen.
 */
export function MaskIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M1.6 5.6c1.9-.7 3.9-1 6.4-1s4.5.3 6.4 1c.3 2.2-.2 3.9-1.2 4.8-1.1 1-2.7.9-3.6.1-.6-.5-.9-1.1-1.6-1.1s-1 .6-1.6 1.1c-.9.8-2.5.9-3.6-.1-1-.9-1.5-2.6-1.2-4.8Z" />
    </Icon>
  );
}

/**
 * Smart Scan. A four-point star with a smaller companion — the only glyph in
 * this set that names an action rather than a place, which is what Smart Scan
 * is: the other six sidebar rows are somewhere on your disk, and this one is a
 * gesture across them.
 */
export function SparkleIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M6.4 1.7 7.6 5 10.9 6.2 7.6 7.4 6.4 10.7 5.2 7.4 1.9 6.2 5.2 5Z" />
      <path d="M11.6 9.1 12.3 11 14.2 11.7 12.3 12.4 11.6 14.3 10.9 12.4 9 11.7 10.9 11Z" />
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
  role,
  label,
}: {
  children: ReactNode;
  className?: string;
  /** `"list"` when the rows are items, so assistive tech gets a count. */
  role?: "list";
  label?: string;
}) {
  return (
    <div
      role={role}
      aria-label={label}
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

/**
 * macOS is withholding something, and here is the way to fix it.
 *
 * Shared rather than owned by the Clean screen because two screens run the same
 * probe over the same roots. A second copy would be a second wording of the same
 * fact, and the two would drift — the wrong direction for a notice whose whole
 * job is to say precisely what is missing from a figure.
 *
 * Fires only when the *permission probe* names a gated root. An ordinary
 * unreadable directory is a different notice: this one promises a remedy
 * (grant Full Disk Access) that would not help there.
 */
export function AccessNotice({ perms }: { perms: Permissions }) {
  const [opening, setOpening] = useState(false);
  const missing = [
    !perms.trash_readable ? "the Trash" : null,
    !perms.containers_readable ? "sandboxed app caches" : null,
  ].filter(Boolean);

  return (
    <div className="mb-5 flex items-start gap-3 rounded-card border border-cat-trashes/30 bg-cat-trashes/[.07] px-4 py-3">
      <span className="text-cat-trashes mt-0.5 flex-none">
        <LockIcon size={16} />
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-body font-medium">
          This scan may be under-reporting
        </p>
        <p className="text-muted mt-1 text-caption leading-relaxed">
          macOS is withholding {missing.join(" and ")} until you grant Full Disk
          Access, so anything in {missing.length === 1 ? "it" : "them"} is
          missing from the total above. Nothing else is affected, and the
          figures shown are still real.
        </p>
      </div>
      <button
        onClick={() => {
          setOpening(true);
          void call("open_privacy_settings").finally(() => setOpening(false));
        }}
        disabled={opening}
        className="shrink-0 rounded-control border border-border bg-surface2 px-3 py-1.5 text-caption font-medium text-text transition-colors duration-fast ease-mac hover:border-borderStrong disabled:opacity-50"
      >
        {opening ? "Opening\u2026" : "Open Settings"}
      </button>
    </div>
  );
}
