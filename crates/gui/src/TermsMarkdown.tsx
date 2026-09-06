import type { ReactNode } from "react";

/**
 * A deliberately tiny Markdown renderer, for one document: `TERMS.md`.
 *
 * The terms are compiled into the binary and served verbatim by `terms_text`,
 * which is the property that makes them trustworthy — the text shown is
 * necessarily the text the build was made from. Rendering them as raw source,
 * though, put `#`, `**`, `---` and `[x](y)` in front of the user on the first
 * screen of the app. For a document whose entire point is "this is the exact
 * contract", presenting it as unparsed source was self-defeating.
 *
 * **No dependency and no `dangerouslySetInnerHTML`.** This emits React
 * elements, so there is no HTML-injection surface at all — which matters
 * because it keeps being true if the terms ever gain a section quoting
 * something odd.
 *
 * It covers exactly what `TERMS.md` uses and nothing else: ATX headings, bold,
 * inline code, links (rendered as plain text — the app has no general
 * URL-opening capability and a dead blue link would be a lie), horizontal
 * rules, blockquotes, bullet and numbered lists, and tables reduced to their
 * rows. Anything unrecognised falls through as text rather than disappearing,
 * because silently dropping a clause from a contract is the one failure this
 * must not have.
 */
export default function TermsMarkdown({ source }: { source: string }) {
  return <>{blocks(source)}</>;
}

function blocks(source: string): ReactNode[] {
  const out: ReactNode[] = [];
  // `TERMS.md` is hard-wrapped at ~76 columns. Joining wrapped lines back into
  // paragraphs is what stops the rendered text double-wrapping into rags.
  const paragraphs = source.replace(/\r\n/g, "\n").split(/\n{2,}/);

  paragraphs.forEach((raw, i) => {
    const block = raw.trim();
    if (!block) return;
    const key = `b${i}`;

    if (/^-{3,}$/.test(block)) {
      out.push(<hr key={key} className="my-4 border-separator" />);
      return;
    }

    const heading = /^(#{1,6})\s+(.*)$/.exec(block);
    if (heading) {
      const depth = heading[1].length;
      out.push(
        <p
          key={key}
          className={
            depth <= 2
              ? "text-text mb-1.5 mt-5 text-emph font-semibold first:mt-0"
              : "text-text mb-1 mt-4 text-body font-semibold first:mt-0"
          }
        >
          {inline(heading[2])}
        </p>,
      );
      return;
    }

    const lines = block.split("\n");

    if (lines.every((l) => /^>\s?/.test(l))) {
      out.push(
        <p
          key={key}
          className="text-muted my-3 border-l-2 border-warning/50 pl-3 text-caption leading-relaxed"
        >
          {inline(lines.map((l) => l.replace(/^>\s?/, "")).join(" "))}
        </p>,
      );
      return;
    }

    // Tables carry real content in TERMS.md's sibling documents; reduce them to
    // their cells rather than dropping them.
    if (lines.length > 1 && lines.every((l) => l.trim().startsWith("|"))) {
      const rows = lines.filter((l) => !/^\s*\|[\s|:-]+\|\s*$/.test(l));
      out.push(
        <ul key={key} className="my-2 space-y-1">
          {rows.map((row, r) => (
            <li key={r} className="text-muted text-caption leading-relaxed">
              {inline(
                row
                  .replace(/^\||\|$/g, "")
                  .split("|")
                  .map((c) => c.trim())
                  .filter(Boolean)
                  .join(" — "),
              )}
            </li>
          ))}
        </ul>,
      );
      return;
    }

    if (lines.some((l) => /^\s*(?:[-*]|\d+\.)\s+/.test(l))) {
      const items = groupListItems(lines);
      out.push(
        <ul key={key} className="my-2 space-y-1.5 pl-4">
          {items.map((item, n) => (
            <li
              key={n}
              className="text-muted list-disc text-caption leading-relaxed marker:text-subtle"
            >
              {inline(item)}
            </li>
          ))}
        </ul>,
      );
      return;
    }

    out.push(
      <p key={key} className="text-muted my-2 text-caption leading-relaxed">
        {inline(lines.join(" "))}
      </p>,
    );
  });

  return out;
}

/** Re-join wrapped continuation lines onto the bullet they belong to. */
function groupListItems(lines: string[]): string[] {
  const items: string[] = [];
  for (const line of lines) {
    const start = /^\s*(?:[-*]|\d+\.)\s+(.*)$/.exec(line);
    if (start) items.push(start[1]);
    else if (items.length) items[items.length - 1] += ` ${line.trim()}`;
    else items.push(line.trim());
  }
  return items;
}

/**
 * Bold, inline code and links, in one pass.
 *
 * Links become their label only. The app grants no general URL-opening
 * permission by design, so anything styled as a link here could not be
 * followed — and a control that looks clickable and is not is exactly the kind
 * of small lie this project avoids elsewhere.
 */
function inline(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  const pattern = /\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\([^)]*\)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let k = 0;

  while ((m = pattern.exec(text)) !== null) {
    if (m.index > last) out.push(text.slice(last, m.index));
    if (m[1] !== undefined) {
      out.push(
        <b key={k++} className="text-text font-semibold">
          {m[1]}
        </b>,
      );
    } else if (m[2] !== undefined) {
      out.push(
        <code key={k++} className="text-text font-mono text-[11px]">
          {m[2]}
        </code>,
      );
    } else {
      out.push(m[3]);
    }
    last = pattern.lastIndex;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}
